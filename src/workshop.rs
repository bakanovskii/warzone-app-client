//! Original XMLRecipes crafting/training; transactions never execute guest code.
use crate::{
    Scene,
    entity_guid::entity_guid_from_words,
    production::{Command, ProductionSource, State, adjust, number, probability},
    systems::Award,
    world::{NE, NW, SE, SW},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Recipe {
    pub id: u32,
    pub name: String,
    pub seconds: i64,
    pub min_level: i64,
    pub research: u32,
    pub unlock_price: i64,
    pub cost: BTreeMap<String, i64>,
    pub items: BTreeMap<u32, i64>,
    pub output: u32,
    pub experience: i64,
    pub probability: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub recipes: BTreeMap<u32, Recipe>,
    pub buildings: BTreeMap<u32, Vec<u32>>,
    pub population: BTreeMap<u32, i64>,
}
impl Recipe {
    pub fn unit(&self) -> bool {
        (2000..2999).contains(&self.output)
    }
}
impl State {
    pub fn workshop_choice(&self, scene: &Scene, uid: &str, index: usize) -> Option<Recipe> {
        let c = scene.workshop.as_ref()?;
        let v = self.entities.get(uid)?;
        let ids = c.buildings.get(&(number(v, "type") as u32))?;
        let id = if number(v, "productionType") != 0 {
            number(v, "productionType") as u32
        } else {
            *ids.get(index % ids.len().max(1))?
        };
        let r = c.recipes.get(&id)?;
        let sequence = if number(v, "productionType") == 0 {
            self.revision + 1
        } else {
            self.workshop_origin_sequence(uid, id, number(v, "productionStartTime"), u64::MAX)
        };
        Some(self.upgraded_recipe(scene, r, sequence))
    }
    pub fn workshop_command(&self, scene: &Scene, uid: &str, index: usize) -> Option<Command> {
        if self.production_gathering(uid) {
            return None;
        }
        let r = self.workshop_choice(scene, uid, index)?;
        let v = self.entities.get(uid)?;
        Some(if number(v, "productionType") == 0 {
            Command::Start {
                uid: uid.into(),
                recipe: r.id,
            }
        } else {
            Command::Collect {
                uid: uid.into(),
                recipe: r.id,
                started: number(v, "productionStartTime"),
            }
        })
    }
    fn workshop_recipe(&self, scene: &Scene, uid: &str, id: u32) -> Result<Recipe, String> {
        let v = self.entities.get(uid).ok_or("Здание не найдено")?;
        if number(v, "state") != 3 || v["isDeleted"] == true || v["isInBattle"] == true {
            return Err("Здание сейчас недоступно".into());
        }
        let c = scene.workshop.as_ref().ok_or("Мастерская недоступна")?;
        if !c
            .buildings
            .get(&(number(v, "type") as u32))
            .is_some_and(|v| v.contains(&id))
        {
            return Err("Рецепт не принадлежит зданию".into());
        }
        let r = c.recipes.get(&id).ok_or("Рецепт не найден")?;
        if r.id != id || r.seconds <= 0 || r.experience < 0 {
            return Err("Invalid workshop recipe".into());
        }
        let sequence = if number(v, "productionType") == 0 {
            self.revision + 1
        } else {
            self.workshop_origin_sequence(uid, id, number(v, "productionStartTime"), u64::MAX)
        };
        Ok(self.upgraded_recipe(scene, r, sequence))
    }
    fn workshop_clock(&self, scene: &Scene, wall: i64) -> Result<i64, String> {
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        Ok(now)
    }
    pub fn workshop_access(&self, r: &Recipe) -> Result<(), String> {
        let s = self.systems.as_ref().ok_or("Missing native preferences")?;
        if r.research != 0 && !s.flag(&format!("Research-{}", r.research)) {
            return Err(format!("Нужно исследование {}", r.research));
        }
        if self.resources["level"] < r.min_level && !s.flag(&format!("RecipeUnlock-{}", r.id)) {
            return Err(format!("Нужен уровень {}", r.min_level));
        }
        Ok(())
    }
    pub fn population(&self, scene: &Scene) -> (i64, i64) {
        let Some(c) = &scene.workshop else {
            return (0, 0);
        };
        let mut used = 0;
        let mut maximum = 0;
        let mut amounts = std::collections::BTreeMap::new();
        for v in self.entities.values().filter(|v| v["isDeleted"] != true) {
            let id = number(v, "type") as u32;
            let unit = (2000..2999).contains(&id);
            let capacity = (1000..1999).contains(&id) && number(v, "state") == 3;
            if !unit && !capacity {
                continue;
            }
            let amount = *amounts.entry(id).or_insert_with(|| {
                self.upgraded_stat(
                    scene,
                    id,
                    "population",
                    c.population.get(&id).copied().unwrap_or(0),
                )
            });
            if unit {
                used += amount;
            }
            if capacity {
                maximum += amount;
            }
        }
        (used, maximum)
    }
    pub fn unit_ready(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        started: i64,
        wall: i64,
    ) -> Result<Recipe, String> {
        let r = self.workshop_recipe(scene, uid, id)?;
        let now = self.workshop_clock(scene, wall)?;
        let v = &self.entities[uid];
        if !r.unit()
            || number(v, "productionType") != id as i64
            || number(v, "productionStartTime") != started
        {
            return Err("Этот отряд уже размещён или производство изменилось".into());
        }
        if started.checked_add(r.seconds).is_none_or(|end| now <= end) {
            return Err("Обучение ещё не завершено".into());
        }
        let (used, max) = self.population(scene);
        let need = scene
            .workshop
            .as_ref()
            .unwrap()
            .population
            .get(&r.output)
            .copied()
            .ok_or("Missing unit population")?;
        let need = self.upgraded_stat(scene, r.output, "population", need);
        if need > 0 && used + need > max {
            return Err(format!(
                "Не хватает мест для армии: {used}/{max}. Построй палатки"
            ));
        }
        Ok(r)
    }
    pub fn workshop_apply(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        self.apply_workshop(scene, cmd, wall, false, None, entropy)
    }
    pub(crate) fn workshop_after_gather(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        at_ms: Option<i64>,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        self.apply_workshop(scene, cmd, wall, true, at_ms, entropy)
    }
    fn apply_workshop(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        gathered: bool,
        at_ms: Option<i64>,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        if self.production_gathering(cmd.uid()) {
            return Err("Производство уже собирается".into());
        }
        let uid = cmd.uid();
        let id = match cmd {
            Command::Start { recipe, .. } | Command::Collect { recipe, .. } => *recipe,
        };
        let r = self.workshop_recipe(scene, uid, id)?;
        let now = self.workshop_clock(scene, wall)?;
        let v = &self.entities[uid];
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        let mut words = vec![];
        let kind = match cmd {
            Command::Start { .. } => {
                if number(v, "productionType") != 0 {
                    return Err("Производство уже идёт".into());
                }
                self.workshop_access(&r)?;
                if now
                    .checked_add(r.seconds)
                    .is_none_or(|end| end >= i32::MAX as i64)
                {
                    return Err("Таймер вне допустимого диапазона".into());
                }
                for (item, count) in &r.items {
                    if *count < 0
                        || self.inventory.get(&item.to_string()).copied().unwrap_or(0) < *count
                    {
                        return Err(format!("Недостаточно: {}", material_name(scene, *item)));
                    }
                }
                for (key, cost) in &r.cost {
                    if *cost < 0
                        || !["playMoney", "realMoney", "fuel", "iron"].contains(&key.as_str())
                    {
                        return Err("Invalid workshop cost".into());
                    }
                    adjust(&mut next.resources, key, -cost)?;
                    gains.insert(key.clone(), -cost);
                    if key == "realMoney" {
                        let s = next.systems.as_mut().unwrap();
                        s.set(
                            "realMoneyFree".into(),
                            (s.integer("realMoneyFree") - cost).max(0),
                        );
                    }
                }
                for (item, count) in &r.items {
                    adjust(&mut next.inventory, &item.to_string(), -count)?;
                    gains.insert(format!("item:{item}"), -count);
                }
                let e = next.entities.get_mut(uid).unwrap();
                e["productionType"] = id.into();
                e["productionStartTime"] = now.into();
                next.production_progress(scene, "start", id, self.resources["level"], &gains)?;
                "workshop_start"
            }
            Command::Collect { started, .. } => {
                if r.unit() {
                    return Err("Готовый отряд нужно разместить на базе".into());
                }
                if number(v, "productionType") != id as i64
                    || number(v, "productionStartTime") != *started
                {
                    return Err("Это производство уже собрано".into());
                }
                if started.checked_add(r.seconds).is_none_or(|end| now <= end) {
                    return Err("Производство ещё не завершено".into());
                }
                if !next.at_top_experience(scene) {
                    next.award(
                        &Award {
                            kind: "experience".into(),
                            count: r.experience.to_string(),
                            item_id: 0,
                        },
                        scene,
                        now,
                        &mut gains,
                    )?;
                }
                // Same material path as onProductionGatherStart, last XML item
                // overwrites nSpecialProbability (even if it is the XP item).
                let word = entropy()?;
                words.push(word);
                if probability(word, r.probability) {
                    let delta = next.add_special_item(r.output, 1)?;
                    gains.insert(format!("item:{}", r.output), delta);
                }
                let e = next.entities.get_mut(uid).unwrap();
                e["productionType"] = 0.into();
                e["productionStartTime"] = 0.into();
                next.production_progress(
                    scene,
                    if gathered {
                        "gather_collect"
                    } else {
                        "collect"
                    },
                    id,
                    self.resources["level"],
                    &gains,
                )?;
                next.update_level(scene, now, &mut gains)?;
                "workshop_collect"
            }
        };
        next.building_receipt(now, uid, id, kind, gains, words)?;
        next.actions.last_mut().unwrap().at_ms = at_ms;
        next.validate(scene)?;
        Ok((
            next,
            if kind == "workshop_start" {
                format!("Запущено: {}", r.name)
            } else {
                format!("Получено: {}", material_name(scene, r.output))
            },
        ))
    }
    pub fn place_trained_unit(
        &self,
        scene: &Scene,
        source: &ProductionSource,
        id: u32,
        x: i32,
        y: i32,
        direction: u32,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        let r = self.unit_ready(scene, &source.uid, id, source.started, wall)?;
        if ![SE, SW, NE, NW].contains(&direction) {
            return Err("Неверное направление отряда".into());
        }
        self.placement_type(scene, r.output, "", x, y, direction)?;
        let now = self.workshop_clock(scene, wall)?;
        let mut words = [0; 6];
        for v in &mut words {
            *v = entropy()?;
        }
        let uid = entity_guid_from_words(words);
        if self.entities.contains_key(&uid) {
            return Err("Совпадение идентификаторов отрядов".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        let v = json!({"uid":uid,"type":r.output,"x":x+16,"y":y+16,"direction":direction,"state":0,"buildStep":0,"buildStartTime":0,"buildTotalTime":0,"productionType":0,"productionStartTime":0,"isDeleted":false,"isInBattle":false,"fraction":"player"});
        next.entities.insert(uid.clone(), v);
        let factory = next.entities.get_mut(&source.uid).unwrap();
        factory["productionType"] = 0.into();
        factory["productionStartTime"] = 0.into();
        if !next.at_top_experience(scene) {
            next.award(
                &Award {
                    kind: "experience".into(),
                    count: r.experience.to_string(),
                    item_id: 0,
                },
                scene,
                now,
                &mut gains,
            )?;
        }
        if let Some(c) = &scene.progression {
            let s = next.systems.as_mut().unwrap();
            s.event(c, self.resources["level"], 5, r.output)?;
            s.achievement("train_units", 1, false);
            s.set("firstUnitPlacing".into(), 0);
            s.set("FirstUnitPlacing".into(), 0);
            s.set("FirstUnitPlacingDialog".into(), 0);
        }
        next.update_level(scene, now, &mut gains)?;
        next.selected = Some(uid.clone());
        next.building_receipt(now, &uid, id, "unit_train", gains, words.to_vec())?;
        next.actions.last_mut().unwrap().source = Some(source.clone());
        next.validate(scene)?;
        Ok((next, "Отряд размещён на базе".into()))
    }
    pub fn validate_trained_unit(&self, scene: &Scene, uid: &str) -> Result<(), String> {
        let v = &self.entities[uid];
        let c = scene.workshop.as_ref().ok_or("Missing training rules")?;
        let id = number(v, "type") as u32;
        let receipts: Vec<_> = self
            .actions
            .iter()
            .filter(|a| a.kind == "unit_train" && a.uid == uid)
            .collect();
        if receipts.len() != 1 {
            return Err("Unit has no unique training receipt".into());
        }
        let a = receipts[0];
        let r = c
            .recipes
            .get(&a.recipe)
            .filter(|r| r.unit() && r.output == id)
            .ok_or("Invalid unit recipe")?;
        let source = a.source.as_ref().ok_or("Missing unit production source")?;
        let sequence = self.workshop_origin_sequence(&source.uid, r.id, source.started, a.sequence);
        let r = self.upgraded_recipe(scene, r, sequence);
        let factory = self
            .entities
            .get(&source.uid)
            .ok_or("Missing source factory")?;
        if !c
            .buildings
            .get(&(number(factory, "type") as u32))
            .is_some_and(|ids| ids.contains(&r.id))
            || source
                .started
                .checked_add(r.seconds)
                .is_none_or(|end| a.at <= end)
            || a.at > self.last_action_time
            || source.started <= 0
            || a.entropy.len() != 6
            || entity_guid_from_words(a.entropy.clone().try_into().unwrap()) != uid
            || v["uid"] != uid
            || number(v, "state") != 0
            || v["fraction"] != "player"
            || v["isInBattle"] != serde_json::Value::Bool(self.unit_in_mission(uid))
            || !v["isDeleted"].is_boolean()
            || number(v, "productionType") != 0
            || number(v, "buildTotalTime") != 0
            || ![SE, SW, NE, NW].contains(&(number(v, "direction") as u32))
        {
            return Err("Invalid trained unit state/origin".into());
        }
        let original_start = self.production_origin(&source.uid, r.id, source.started, a.sequence);
        let imported = scene
            .entities
            .iter()
            .find(|e| e.uid == source.uid)
            .is_some_and(|e| {
                number(&e.archived, "productionType") == r.id as i64
                    && number(&e.archived, "productionStartTime") == original_start
            });
        let starts = self.actions.iter().any(|b| {
            b.uid == source.uid
                && b.recipe == r.id
                && b.kind == "workshop_start"
                && b.at == original_start
                && b.sequence < a.sequence
        });
        if !imported && !starts {
            return Err("Unit source has no production start".into());
        }
        if self
            .actions
            .iter()
            .filter(|b| {
                b.kind == "unit_train"
                    && b.source
                        .as_ref()
                        .is_some_and(|s| s.uid == source.uid && s.started == source.started)
            })
            .count()
            != 1
        {
            return Err("Training source consumed more than once".into());
        }
        if ["x", "y"]
            .iter()
            .any(|k| v[k].as_i64().is_none_or(|n| !(0..128).contains(&n)))
        {
            return Err("Invalid trained unit position".into());
        }
        if v["isDeleted"] != true {
            self.placement_type(
                scene,
                id,
                uid,
                number(v, "x") as i32 - 16,
                number(v, "y") as i32 - 16,
                number(v, "direction") as u32,
            )?;
        }
        self.catalog_entity(scene, v, None)?;
        Ok(())
    }
    pub fn validate_workshop(&self, scene: &Scene) -> Result<(), String> {
        self.validate_workshop_unlocks(scene)?;
        let c = scene.workshop.as_ref().unwrap();
        let mut started = HashSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "workshop_start") {
            if !started.insert((&a.uid, a.at)) || !c.recipes.contains_key(&a.recipe) {
                return Err("Invalid/duplicate workshop start".into());
            }
        }
        Ok(())
    }
    pub fn workshop_panel(
        &self,
        scene: &Scene,
        uid: &str,
        index: usize,
        wall: i64,
    ) -> Option<(String, String)> {
        let v = self.entities.get(uid)?;
        let id = number(v, "type") as u32;
        if (2000..2999).contains(&id) && scene.workshop.is_some() {
            let (used, max) = self.population(scene);
            let need = scene
                .workshop
                .as_ref()?
                .population
                .get(&id)
                .copied()
                .unwrap_or(0);
            let need = self.upgraded_stat(scene, id, "population", need);
            return Some((
                format!("Отряд на базе · Армия {used}/{max} · Занимает {need} мест"),
                String::new(),
            ));
        }
        let r = self.workshop_choice(scene, uid, index)?;
        let v = &self.entities[uid];
        if number(v, "productionType") != 0 {
            let left = number(v, "productionStartTime") + r.seconds
                - self.clock(scene.economy.as_ref()?, wall).unwrap_or(0)
                + 1;
            return Some((
                format!(
                    "{} · {}",
                    r.name,
                    if left > 0 {
                        format!("осталось {}:{:02}", left / 60, left % 60)
                    } else {
                        "Готово".into()
                    }
                ),
                if left > 0 {
                    "Подождать".into()
                } else if r.unit() {
                    "Разместить отряд".into()
                } else {
                    "Собрать материал".into()
                },
            ));
        }
        let cost = r
            .cost
            .iter()
            .map(|(k, n)| format!("{n} {}", crate::production::resource_name(k)))
            .chain(
                r.items
                    .iter()
                    .map(|(id, n)| format!("{n} {}", material_name(scene, *id))),
            )
            .collect::<Vec<_>>()
            .join(", ");
        let (used, max) = self.population(scene);
        let access = self.workshop_access(&r).err();
        Some((
            format!(
                "{} · {} мин · {cost}{}",
                r.name,
                r.seconds / 60,
                if r.unit() {
                    format!(" · Армия {used}/{max}")
                } else {
                    String::new()
                }
            ),
            access.unwrap_or_else(|| "Начать".into()),
        ))
    }
}

fn material_name(scene: &Scene, id: u32) -> String {
    scene
        .progression
        .as_ref()
        .and_then(|c| c.special_items.get(&id))
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "Материал".into())
}

impl State {
    /// LaboratoryDialog.onUnlockAccess0x1ac371 pays once and sets Recipes flag.
    pub fn unlock_workshop_recipe(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let r = self.workshop_recipe(scene, uid, id)?;
        let now = self.workshop_clock(scene, wall)?;
        if self.workshop_access(&r).is_ok() {
            return Err("Доступ к рецепту уже открыт".into());
        }
        if r.research != 0 && !self.research_ready(r.research) {
            return Err("Сначала выполни исследование".into());
        }
        if r.unlock_price <= 0 {
            return Err("У рецепта нет цены открытия".into());
        }
        let mut next = self.clone();
        next.charge_cash(r.unlock_price)?;
        next.systems
            .as_mut()
            .ok_or("Missing source preferences")?
            .set(format!("RecipeUnlock-{id}"), 1);
        next.building_receipt(
            now,
            uid,
            id,
            "workshop_unlock",
            BTreeMap::from([("realMoney".into(), -r.unlock_price)]),
            vec![],
        )?;
        next.validate(scene)?;
        Ok((next, "Доступ к рецепту открыт".into()))
    }
    fn validate_workshop_unlocks(&self, scene: &Scene) -> Result<(), String> {
        let mut seen = HashSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "workshop_unlock") {
            let r = scene
                .workshop
                .as_ref()
                .and_then(|c| c.recipes.get(&a.recipe))
                .ok_or("Unknown unlocked workshop recipe")?;
            if !seen.insert(a.recipe)
                || r.unlock_price <= 0
                || a.gains != BTreeMap::from([("realMoney".into(), -r.unlock_price)])
                || !a.entropy.is_empty()
                || !self
                    .systems
                    .as_ref()
                    .is_some_and(|s| s.flag(&format!("RecipeUnlock-{}", a.recipe)))
                || !self.entities.get(&a.uid).is_some_and(|e| {
                    scene
                        .workshop
                        .as_ref()
                        .unwrap()
                        .buildings
                        .get(&(number(e, "type") as u32))
                        .is_some_and(|ids| ids.contains(&a.recipe))
                })
            {
                return Err("Invalid workshop unlock receipt".into());
            }
        }
        Ok(())
    }
}
