//! ResearchDialogUnit -> SceneMain research transactions. No guest/server calls.
use crate::{
    Scene,
    production::{Command, State, adjust, number},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Research {
    pub id: u32,
    pub name: String,
    pub seconds: i64,
    pub unit: u32,
    pub min_level: i64,
    pub parent: u32,
    pub entity: u32,
    pub unlock_price: i64,
    pub cost: BTreeMap<String, i64>,
    pub items: BTreeMap<u32, i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub items: BTreeMap<u32, Research>,
}
impl State {
    pub fn research_ready(&self, id: u32) -> bool {
        id == 0
            || self
                .systems
                .as_ref()
                .is_some_and(|s| s.flag(&format!("Research-{id}")))
    }
    pub fn research_choice<'a>(
        &self,
        scene: &'a Scene,
        uid: &str,
        index: usize,
    ) -> Option<&'a Research> {
        let v = self.entities.get(uid)?;
        if number(v, "type") != 1020 {
            return None;
        }
        let c = scene.research.as_ref()?;
        let active = number(v, "productionType");
        if active != 0 {
            return c.items.get(&(active as u32));
        }
        c.items.values().nth(index % c.items.len().max(1))
    }
    pub fn research_command(&self, scene: &Scene, uid: &str, index: usize) -> Option<Command> {
        if self.production_gathering(uid) {
            return None;
        }
        let r = self.research_choice(scene, uid, index)?;
        let v = &self.entities[uid];
        if number(v, "productionType") == 0 {
            Some(Command::Start {
                uid: uid.into(),
                recipe: r.id,
            })
        } else {
            Some(Command::Collect {
                uid: uid.into(),
                recipe: r.id,
                started: number(v, "productionStartTime"),
            })
        }
    }
    pub fn research_access(&self, scene: &Scene, r: &Research) -> Result<(), String> {
        // The detail's normal Start callback checks the parent even when
        // an imported paid-access flag bypassed the list's restrictions.
        if !self.research_ready(r.parent) {
            return Err("Сначала заверши предыдущее исследование".into());
        }
        self.research_dialog_access(scene, r)
    }
    /// ResearchDialog.updateUnit: payment bypasses LEVEL only. The access
    /// button stays hidden while a parent research or required factory is absent.
    pub fn can_buy_research_access(&self, r: &Research) -> bool {
        !self.research_ready(r.id)
            && self.resources["level"] < r.min_level
            && self.research_ready(r.parent)
            && !self
                .systems
                .as_ref()
                .is_some_and(|s| s.flag(&format!("ResearchUnlock-{}", r.id)))
            && (r.entity == 0
                || self.entities.values().any(|v| {
                    number(v, "type") == r.entity as i64
                        && [0, 3].contains(&number(v, "state"))
                        && v["isDeleted"] != true
                }))
    }
    pub fn research_dialog_access(&self, scene: &Scene, r: &Research) -> Result<(), String> {
        if self.research_ready(r.id) {
            return Err("Уже исследовано".into());
        }
        if self
            .systems
            .as_ref()
            .is_some_and(|s| s.flag(&format!("ResearchUnlock-{}", r.id)))
        {
            return Ok(());
        }
        if !self.research_ready(r.parent) {
            return Err("Сначала заверши предыдущее исследование".into());
        }
        // UserProfile.isBuildingExist counts completed OR damaged structures,
        // but never unfinished/deleted ones.
        if r.entity != 0
            && !self.entities.values().any(|v| {
                number(v, "type") == r.entity as i64
                    && [0, 3].contains(&number(v, "state"))
                    && v["isDeleted"] != true
            })
        {
            let name = scene
                .world
                .as_ref()
                .and_then(|c| c.catalog.get(&r.entity))
                .map_or("нужное здание", |d| d.name.as_str());
            return Err(format!("Сначала построй: {name}"));
        }
        if self.resources["level"] < r.min_level
            && !self
                .systems
                .as_ref()
                .is_some_and(|s| s.flag(&format!("ResearchUnlock-{}", r.id)))
        {
            return Err(format!("Нужен уровень {}", r.min_level));
        }
        Ok(())
    }
    pub fn research_apply(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
    ) -> Result<(Self, String), String> {
        self.apply_research(scene, cmd, wall, false, None)
    }
    pub(crate) fn research_after_gather(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        at_ms: Option<i64>,
    ) -> Result<(Self, String), String> {
        self.apply_research(scene, cmd, wall, true, at_ms)
    }
    fn apply_research(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        gathered: bool,
        at_ms: Option<i64>,
    ) -> Result<(Self, String), String> {
        if self.production_gathering(cmd.uid()) {
            return Err("Исследование уже получается".into());
        }
        let uid = cmd.uid();
        let id = match cmd {
            Command::Start { recipe, .. } | Command::Collect { recipe, .. } => *recipe,
        };
        let v = self.entities.get(uid).ok_or("Научный центр не найден")?;
        if number(v, "type") != 1020
            || number(v, "state") != 3
            || v["isDeleted"] == true
            || v["isInBattle"] == true
        {
            return Err("Научный центр сейчас недоступен".into());
        }
        let r = scene
            .research
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .ok_or("Исследование не найдено")?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        let (kind, message) = match cmd {
            Command::Start { .. } => {
                self.research_access(scene, r)?;
                if number(v, "productionType") != 0
                    || self
                        .entities
                        .values()
                        .any(|e| e["isDeleted"] != true && number(e, "productionType") == id as i64)
                {
                    return Err("Исследование уже идёт".into());
                }
                if r.seconds <= 0
                    || now
                        .checked_add(r.seconds)
                        .is_none_or(|end| end >= i32::MAX as i64)
                {
                    return Err("Invalid research timer".into());
                }
                for (item, count) in &r.items {
                    if *count < 0
                        || self.inventory.get(&item.to_string()).copied().unwrap_or(0) < *count
                    {
                        return Err("Недостаточно материалов: изготовь их в лаборатории".into());
                    }
                }
                for (k, n) in &r.cost {
                    if *n < 0 || !["playMoney", "fuel", "iron"].contains(&k.as_str()) {
                        return Err("Invalid research cost".into());
                    }
                    adjust(&mut next.resources, k, -n)?;
                    gains.insert(k.clone(), -n);
                }
                for (item, count) in &r.items {
                    adjust(&mut next.inventory, &item.to_string(), -count)?;
                    gains.insert(format!("item:{item}"), -count);
                }
                let e = next.entities.get_mut(uid).unwrap();
                e["productionType"] = id.into();
                e["productionStartTime"] = now.into();
                if let Some(c) = &scene.progression {
                    next.systems
                        .as_mut()
                        .unwrap()
                        .event(c, self.resources["level"], 51, id)?;
                }
                ("research_start", format!("Начато исследование: {}", r.name))
            }
            Command::Collect { started, .. } => {
                if number(v, "productionType") != id as i64
                    || number(v, "productionStartTime") != *started
                    || self.research_ready(id)
                {
                    return Err("Исследование уже завершено или изменилось".into());
                }
                if started.checked_add(r.seconds).is_none_or(|end| now <= end) {
                    return Err("Исследование ещё не завершено".into());
                }
                let e = next.entities.get_mut(uid).unwrap();
                e["productionType"] = 0.into();
                e["productionStartTime"] = 0.into();
                let s = next.systems.as_mut().unwrap();
                s.set(format!("Research-{id}"), 1);
                s.achievement("complete_researches", 1, false);
                if let Some(c) = scene.progression.as_ref().filter(|_| !gathered) {
                    s.event(c, self.resources["level"], 52, id)?;
                }
                // Original research block + falling item unlock training/quests,
                // but have NO XP/resource reward. Never invent one here.
                (
                    "research_finish",
                    format!("Исследовано: {} · обучение открыто", r.name),
                )
            }
        };
        next.building_receipt(now, uid, id, kind, gains, vec![])?;
        next.actions.last_mut().unwrap().at_ms = at_ms;
        next.validate(scene)?;
        Ok((next, message))
    }
    /// ResearchDialog::onResearchUnitBuy consumes available inputs and pays
    /// for the remainder plus the complete original timer, atomically.
    pub fn buy_research(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let v = self.entities.get(uid).ok_or("Научный центр не найден")?;
        if number(v, "type") != 1020
            || number(v, "state") != 3
            || v["isDeleted"] == true
            || v["isInBattle"] == true
        {
            return Err("Научный центр сейчас недоступен".into());
        }
        if number(v, "productionType") != 0 {
            return Err("В научном центре уже идёт исследование".into());
        }
        let r = scene
            .research
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .ok_or("Исследование не найдено")?;
        self.research_dialog_access(scene, r)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let quote = self.research_quote(scene, r)?;
        let mut next = self.clone();
        next.charge_cash(quote.cash)?;
        let mut gains = BTreeMap::from([("realMoney".into(), -quote.cash)]);
        for (key, used) in quote.consume {
            if let Some(item) = key.strip_prefix("item:") {
                adjust(&mut next.inventory, item, -used)?;
            } else {
                adjust(&mut next.resources, &key, -used)?;
            }
            gains.insert(key, -used);
        }
        let s = next.systems.as_mut().ok_or("Missing research state")?;
        s.set(format!("Research-{id}"), 1);
        s.achievement("complete_researches", 1, false);
        if let Some(c) = &scene.progression {
            for event in [51, 52] {
                s.event(c, self.resources["level"], event, id)?;
            }
        }
        next.building_receipt(now, uid, id, "research_buy", gains, vec![])?;
        next.validate(scene)?;
        Ok((
            next,
            format!("Исследовано: {}", scene.item_name(r.unit, &r.name)),
        ))
    }
    fn validate_research_purchase(
        &self,
        scene: &Scene,
        a: &crate::production::Action,
        r: &Research,
    ) -> Result<(), String> {
        let c = scene
            .pricing
            .as_ref()
            .ok_or("Missing paid research configuration")?;
        if self
            .entities
            .get(&a.uid)
            .is_none_or(|e| number(e, "type") != 1020)
            || a.gains.len() != 1 + r.cost.len() + r.items.len()
        {
            return Err("Invalid paid research receipt".into());
        }
        let mut price = c.time(r.seconds)?;
        for (key, need) in &r.cost {
            let used = a
                .gains
                .get(key)
                .copied()
                .and_then(i64::checked_neg)
                .filter(|n| *n <= *need)
                .ok_or("Invalid paid research resource")?;
            if used < 0 {
                return Err("Invalid paid research resource".into());
            }
            price = price
                .checked_add(c.resource(key, need - used)?)
                .ok_or("Price overflow")?;
        }
        for (id, need) in &r.items {
            let used = a
                .gains
                .get(&format!("item:{id}"))
                .copied()
                .and_then(i64::checked_neg)
                .filter(|n| *n <= *need)
                .ok_or("Invalid paid research material")?;
            if used < 0 {
                return Err("Invalid paid research material".into());
            }
            price = price
                .checked_add(
                    crate::pricing::special_price(scene, *id)?
                        .checked_mul(need - used)
                        .ok_or("Price overflow")?,
                )
                .ok_or("Price overflow")?;
        }
        if a.gains.get("realMoney") != Some(&-price) {
            return Err("Paid research price does not match consumed inputs".into());
        }
        Ok(())
    }
    pub fn unlock_research(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let v = self.entities.get(uid).ok_or("Научный центр не найден")?;
        if number(v, "type") != 1020
            || number(v, "state") != 3
            || number(v, "productionType") != 0
            || v["isDeleted"] == true
            || v["isInBattle"] == true
        {
            return Err("Научный центр сейчас недоступен".into());
        }
        let r = scene
            .research
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .ok_or("Исследование не найдено")?;
        if !self.can_buy_research_access(r) {
            return Err("Платный доступ сейчас недоступен: проверь уровень, предыдущее исследование и здание".into());
        }
        if r.unlock_price <= 0 {
            return Err("Нет исходной цены доступа".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        next.charge_cash(r.unlock_price)?;
        next.systems
            .as_mut()
            .unwrap()
            .set(format!("ResearchUnlock-{id}"), 1);
        next.building_receipt(
            now,
            uid,
            id,
            "research_unlock",
            BTreeMap::from([("realMoney".into(), -r.unlock_price)]),
            vec![],
        )?;
        next.validate(scene)?;
        Ok((
            next,
            format!("Открыт доступ: {}", scene.item_name(r.unit, &r.name)),
        ))
    }
    pub fn validate_research(&self, scene: &Scene) -> Result<(), String> {
        let c = scene.research.as_ref().unwrap();
        let mut completed = HashSet::new();
        let mut unlocked = HashSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "research_unlock") {
            let r = c
                .items
                .get(&a.recipe)
                .ok_or("Unknown research access receipt")?;
            if !unlocked.insert(a.recipe)
                || !self
                    .systems
                    .as_ref()
                    .is_some_and(|s| s.flag(&format!("ResearchUnlock-{}", a.recipe)))
                || self
                    .entities
                    .get(&a.uid)
                    .is_none_or(|e| number(e, "type") != 1020)
                || !a.entropy.is_empty()
                || r.unlock_price <= 0
                || a.gains != BTreeMap::from([("realMoney".into(), -r.unlock_price)])
            {
                return Err("Invalid research access receipt".into());
            }
        }

        for a in self
            .actions
            .iter()
            .filter(|a| matches!(a.kind.as_str(), "research_finish" | "research_buy"))
        {
            if !c.items.contains_key(&a.recipe)
                || !completed.insert(a.recipe)
                || !self.research_ready(a.recipe)
                || !a.entropy.is_empty()
                || (a.kind == "research_finish" && !a.gains.is_empty())
                || a.at > self.last_action_time
            {
                return Err("Invalid native research receipt".into());
            }
            let r = &c.items[&a.recipe];
            if a.kind == "research_buy" {
                self.validate_research_purchase(scene, a, r)?;
                continue;
            }
            let start = self
                .actions
                .iter()
                .rfind(|b| {
                    b.kind == "research_start"
                        && b.recipe == a.recipe
                        && b.uid == a.uid
                        && b.sequence < a.sequence
                })
                .map(|b| b.at)
                .or_else(|| {
                    scene
                        .entities
                        .iter()
                        .find(|e| {
                            e.uid == a.uid
                                && number(&e.archived, "productionType") == a.recipe as i64
                        })
                        .map(|e| number(&e.archived, "productionStartTime"))
                });
            if start.is_none_or(|start| {
                self.production_forced_start(&a.uid, a.recipe, start, a.sequence)
                    .checked_add(r.seconds)
                    .is_none_or(|end| a.at <= end)
            }) {
                return Err("Research completion has no valid start".into());
            }
        }
        Ok(())
    }
    pub fn research_panel(
        &self,
        scene: &Scene,
        uid: &str,
        index: usize,
        wall: i64,
    ) -> Option<(String, String)> {
        let r = self.research_choice(scene, uid, index)?;
        let e = &self.entities[uid];
        if number(e, "productionType") != 0 {
            let left = number(e, "productionStartTime") + r.seconds
                - self.clock(scene.economy.as_ref()?, wall).ok()?
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
                } else {
                    "Завершить исследование".into()
                },
            ));
        }
        let cost = r
            .cost
            .iter()
            .map(|(k, n)| format!("{n} {}", crate::production::resource_name(k)))
            .chain(r.items.iter().map(|(id, n)| {
                format!(
                    "{n} {}",
                    scene
                        .progression
                        .as_ref()
                        .and_then(|c| c.special_items.get(id))
                        .map_or("материала", |v| v.name.as_str())
                )
            }))
            .collect::<Vec<_>>()
            .join(", ");
        Some((
            format!("{} · {} мин · {cost}", r.name, r.seconds / 60),
            self.research_access(scene, r)
                .err()
                .unwrap_or_else(|| "Начать исследование".into()),
        ))
    }
}
