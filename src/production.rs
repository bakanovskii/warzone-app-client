//! Resource production recovered from XMLRecipes, Building and SceneMain.
//! No guest calls. Unsupported families stay archived and cannot be mutated.
use crate::Scene;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reward {
    pub count: i64,
    pub probability: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Recipe {
    pub id: u32,
    pub seconds: i64,
    pub min_level: i64,
    #[serde(default)]
    pub unlock_price: Option<i64>,
    pub cost: BTreeMap<String, i64>,
    pub outputs: BTreeMap<String, Reward>,
    pub experience: i64,
    pub special_item: Option<u32>,
    pub special_probability: i32,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Building {
    pub recipes: Vec<u32>,
    pub storage: BTreeMap<String, i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub recipes: BTreeMap<u32, Recipe>,
    pub buildings: BTreeMap<u32, Building>,
    pub level_experience: BTreeMap<i64, i64>,
    pub inventory: BTreeMap<String, i64>,
    pub time_shift: i64,
    pub scope: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct State {
    pub format: String,
    pub seed_sha256: String,
    pub revision: u64,
    pub test_only: bool,
    pub resources: BTreeMap<String, i64>,
    pub inventory: BTreeMap<String, i64>,
    /// All imported entity fields, including currently unsupported productions.
    pub entities: BTreeMap<String, Value>,
    pub camera: [f64; 3],
    pub selected: Option<String>,
    /// Pending semantic events, NOT a claim that quest progression is implemented.
    pub actions: Vec<Action>,
    pub last_action_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub systems: Option<crate::systems::Systems>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<crate::mission::Session>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Action {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shop: Option<crate::shop::AccessReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ProductionSource>,
    pub sequence: u64,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    pub uid: String,
    pub recipe: u32,
    pub kind: String,
    pub gains: BTreeMap<String, i64>,
    pub entropy: Vec<u32>,
    pub quest_events_pending: Vec<(u32, u32)>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductionSource {
    pub uid: String,
    pub started: i64,
}
#[derive(Clone, Debug)]
pub enum Command {
    Start {
        uid: String,
        recipe: u32,
    },
    Collect {
        uid: String,
        recipe: u32,
        started: i64,
    },
}
impl Command {
    pub fn uid(&self) -> &str {
        match self {
            Self::Start { uid, .. } | Self::Collect { uid, .. } => uid,
        }
    }
}
/// SceneMain.checkProbabilityWithValue: uses remainder 101 and INCLUSIVE <=.
/// This deliberately retains the original nonzero success probability at 0.
pub fn probability(word: u32, chance: i32) -> bool {
    (word % 101) as i32 <= chance
}
pub(crate) fn number(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}
pub(crate) fn adjust(map: &mut BTreeMap<String, i64>, key: &str, delta: i64) -> Result<(), String> {
    let new = map
        .get(key)
        .copied()
        .unwrap_or(0)
        .checked_add(delta)
        .ok_or("Слишком большое значение ресурса")?;
    // Resource wraparound/sentinel cases are outside this first audited slice.
    if !(0..i32::MAX as i64).contains(&new) {
        return Err("Значение ресурса вне поддерживаемого диапазона".into());
    }
    map.insert(key.into(), new);
    Ok(())
}
impl State {
    pub fn seed(scene: &Scene, sha: String) -> Result<Self, String> {
        let config = scene
            .economy
            .as_ref()
            .ok_or("This pack has no native production config; export scene-v3")?;
        let mut state = Self {
            format: "warzone-desktop-state-v1".into(),
            seed_sha256: sha,
            revision: 0,
            test_only: false,
            resources: scene
                .resources
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            inventory: config.inventory.clone(),
            entities: scene
                .entities
                .iter()
                .map(|e| (e.uid.clone(), e.archived.clone()))
                .collect(),
            camera: scene.camera,
            selected: None,
            actions: Vec::new(),
            last_action_time: 0,
            systems: None,
            mission: None,
        };
        state.initialize_systems(scene)?;
        state.validate(scene)?;
        Ok(state)
    }
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.format != "warzone-desktop-state-v1"
            || (scene.construction.is_none() && self.entities.len() != scene.entities.len())
            || self.entities.len() > 10000
            || !self.camera.iter().all(|v| v.is_finite())
            || !(0.12..=2.).contains(&self.camera[2])
            || self
                .selected
                .as_ref()
                .is_some_and(|s| !self.entities.contains_key(s))
            || self
                .resources
                .values()
                .chain(self.inventory.values())
                .any(|n| *n < 0 || *n >= i32::MAX as i64)
            || ["playMoney", "realMoney", "fuel", "iron", "level", "exp"]
                .iter()
                .any(|k| !self.resources.contains_key(*k))
        {
            return Err("Invalid native state; refusing to reset your progress".into());
        }
        for e in &scene.entities {
            let v = self.entities.get(&e.uid).ok_or("Missing saved entity")?;
            if e.archived["isDeleted"] == true
                && (v != &e.archived || self.actions.iter().any(|a| a.uid == e.uid))
            {
                return Err("Imported deleted archive was changed or resurrected".into());
            }
            if v["uid"] != e.uid
                || v["type"] != e.type_id
                || (scene.world.is_none()
                    && (v["x"] != e.archived["x"] || v["y"] != e.archived["y"]))
            {
                return Err("Native entity identity/position differs from seed".into());
            }
        }
        if scene.world.is_some() {
            self.validate_world(scene)?;
        }
        if scene.construction.is_some() {
            self.validate_construction(scene)?;
        }
        if scene.lifecycle.is_some() {
            self.validate_lifecycle(scene)?;
        }
        if scene.workshop.is_some() {
            self.validate_workshop(scene)?;
        }
        if scene.research.is_some() {
            self.validate_research(scene)?;
        }
        if scene.pricing.is_some() {
            self.validate_purchases(scene)?;
        }
        self.validate_bank()?;
        self.validate_obstacles(scene)?;
        self.validate_expansions(scene)?;
        self.validate_shop_accesses(scene)?;
        self.validate_upgrades(scene)?;
        self.validate_callback_times()?;
        self.validate_upgrade_gathers(scene)?;
        self.validate_production_gathers(scene)?;
        self.validate_mission(scene)?;
        self.validate_quest_story(scene)?;
        self.validate_achievement_claims(scene)?;
        Ok(())
    }
    pub fn capacity(&self, config: &Config, resource: &str) -> i64 {
        self.entities
            .values()
            .filter(|e| {
                number(e, "state") == 3
                    && e["isDeleted"] != true
                    && (1000..2000).contains(&number(e, "type"))
            })
            .filter_map(|e| config.buildings.get(&(number(e, "type") as u32)))
            .map(|b| b.storage.get(resource).copied().unwrap_or(0))
            .sum()
    }
    pub fn clock(&self, config: &Config, wall: i64) -> Result<i64, String> {
        let now = wall
            .checked_add(config.time_shift)
            .ok_or("Некорректное время")?;
        if now <= 0 || now > i32::MAX as i64 {
            return Err("Время вне проверенного диапазона таймеров".into());
        }
        Ok(now)
    }
    pub fn choice(&self, config: &Config, uid: &str, index: usize) -> Option<u32> {
        let e = self.entities.get(uid)?;
        let current = number(e, "productionType");
        if current != 0 {
            return u32::try_from(current).ok();
        }
        let choices = &config.buildings.get(&(number(e, "type") as u32))?.recipes;
        choices.get(index % choices.len().max(1)).copied()
    }
    pub fn command(&self, config: &Config, uid: &str, index: usize) -> Option<Command> {
        if self.production_gathering(uid) {
            return None;
        }
        let recipe = self.choice(config, uid, index)?;
        if !config.recipes.contains_key(&recipe) {
            return None;
        }
        let e = self.entities.get(uid)?;
        Some(if number(e, "productionType") == 0 {
            Command::Start {
                uid: uid.into(),
                recipe,
            }
        } else {
            Command::Collect {
                uid: uid.into(),
                recipe,
                started: number(e, "productionStartTime"),
            }
        })
    }
    /// Returns a NEW state: even a late validation or entropy error cannot mutate
    /// the current session. The caller publishes it only after an atomic save.
    pub fn apply(
        &self,
        scene: &Scene,
        command: &Command,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        self.apply_resource(scene, command, wall, false, None, entropy)
    }
    pub(crate) fn resource_after_gather(
        &self,
        scene: &Scene,
        command: &Command,
        wall: i64,
        at_ms: Option<i64>,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        self.apply_resource(scene, command, wall, true, at_ms, entropy)
    }
    fn apply_resource(
        &self,
        scene: &Scene,
        command: &Command,
        wall: i64,
        gathered: bool,
        at_ms: Option<i64>,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        if self.production_gathering(command.uid()) {
            return Err("Производство уже собирается".into());
        }
        let config = scene
            .economy
            .as_ref()
            .ok_or("Производство недоступно в этом пакете")?;
        let now = self.clock(config, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад; дождитесь восстановления времени".into());
        }
        let uid = command.uid();
        let e = self.entities.get(uid).ok_or("Здание не найдено")?;
        if number(e, "state") != 3 || e["isDeleted"] == true || e["isInBattle"] == true {
            return Err("Здание сейчас недоступно для производства".into());
        }
        let recipe_id = match command {
            Command::Start { recipe, .. } | Command::Collect { recipe, .. } => *recipe,
        };
        let recipe = config
            .recipes
            .get(&recipe_id)
            .ok_or("Этот вид производства ещё не подключён")?;
        if !(1000..1024).contains(&recipe.id)
            || recipe.id != recipe_id
            || recipe.seconds <= 0
            || recipe.experience < 0
            || !config
                .buildings
                .get(&(number(e, "type") as u32))
                .is_some_and(|b| b.recipes.contains(&recipe_id))
        {
            return Err("Рецепт не принадлежит этому зданию".into());
        }
        let mut next = self.clone();
        next.initialize_systems(scene)?;
        let mut gains = BTreeMap::new();
        let mut words = Vec::new();
        let (kind, quest_events, message) = match command {
            Command::Start { .. } => {
                if [1026, 1028].contains(&(number(e, "type") as u32)) {
                    return Err(
                        "Производство запускается в харвестере, а не на поле кристаллов".into(),
                    );
                }
                if number(e, "productionType") != 0 {
                    return Err("Производство уже запущено".into());
                }
                if self.resources["level"] < recipe.min_level
                    && !next
                        .systems
                        .as_ref()
                        .is_some_and(|s| s.flag(&format!("RecipeUnlock-{recipe_id}")))
                {
                    return Err(format!("Нужен уровень {}", recipe.min_level));
                }
                now.checked_add(recipe.seconds)
                    .filter(|v| *v < i32::MAX as i64)
                    .ok_or("Таймер вне допустимого диапазона")?;
                for (key, cost) in &recipe.cost {
                    if *cost < 0
                        || !["playMoney", "realMoney", "fuel", "iron"].contains(&key.as_str())
                    {
                        return Err("Неподдерживаемая стоимость".into());
                    }
                    if self.resources.get(key).copied().unwrap_or(0) < *cost {
                        return Err(format!("Недостаточно: {}", resource_name(key)));
                    }
                    adjust(&mut next.resources, key, -cost)?;
                }
                let target = next.entities.get_mut(uid).unwrap();
                target["productionType"] = recipe_id.into();
                target["productionStartTime"] = now.into();
                (
                    "start",
                    vec![(3, recipe_id)],
                    "Производство запущено и сохранено".to_string(),
                )
            }
            Command::Collect { started, .. } => {
                if number(e, "productionType") != recipe_id as i64
                    || number(e, "productionStartTime") != *started
                {
                    return Err("Эта награда уже собрана или производство изменилось".into());
                }
                let ready = started
                    .checked_add(recipe.seconds)
                    .ok_or("Некорректный таймер")?;
                // Original isProductionReady is strictly now > end, not >=.
                if now <= ready {
                    return Err(format!("До готовности: {} с", ready - now + 1));
                }
                for key in ["fuel", "iron"] {
                    if gathered {
                        continue;
                    } // Capacity was reserved at gather start.
                    if let Some(output) = recipe.outputs.get(key) {
                        if self.resources[key]
                            .checked_add(output.count)
                            .ok_or("Переполнение ресурса")?
                            > self.capacity(config, key)
                        {
                            return Err(format!(
                                "Склад переполнен: {}. Награда остаётся в здании",
                                resource_name(key)
                            ));
                        }
                    }
                }
                let xp = self.resources["exp"]
                    .checked_add(recipe.experience)
                    .ok_or("Переполнение опыта")?;
                if scene.progression.is_none()
                    && config
                        .level_experience
                        .get(&(self.resources["level"] + 1))
                        .is_some_and(|threshold| xp >= *threshold)
                {
                    return Err(
                        "Повышение уровня пока недоступно; награда остаётся в здании".into(),
                    );
                }
                let experience = if next.at_top_experience(scene) {
                    0
                } else {
                    recipe.experience
                };
                adjust(&mut next.resources, "exp", experience)?;
                gains.insert("exp".into(), experience);
                // Original gather order matters for the entropy transcript.
                for key in ["playMoney", "fuel", "iron"] {
                    if let Some(output) = recipe.outputs.get(key) {
                        if output.count < 0 {
                            return Err("Неверная награда".into());
                        }
                        if output.count != 0 {
                            let word = entropy()?;
                            words.push(word);
                            if probability(word, output.probability) {
                                adjust(&mut next.resources, key, output.count)?;
                                gains.insert(key.into(), output.count);
                            }
                        }
                    }
                }
                if let Some(item) = recipe.special_item {
                    if !(4000..5000).contains(&item) {
                        return Err("Неподдерживаемый предмет".into());
                    }
                    let word = entropy()?;
                    words.push(word);
                    if probability(word, recipe.special_probability) {
                        next.add_special_item(item, 1)?;
                        gains.insert(format!("item:{item}"), 1);
                    }
                }
                let target = next.entities.get_mut(uid).unwrap();
                target["productionType"] = 0.into();
                target["productionStartTime"] = 0.into();
                let rewards = gains
                    .iter()
                    .map(|(k, n)| format!("+{n} {}", resource_name(k)))
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    "collect",
                    vec![(4, recipe_id), (12, 99999)],
                    format!("Собрано: {rewards}. Сохранено"),
                )
            }
        };
        next.production_progress(
            scene,
            if gathered && kind == "collect" {
                "gather_collect"
            } else {
                kind
            },
            recipe_id,
            self.resources["level"],
            &gains,
        )?;
        let level_up = next.update_level(scene, now, &mut gains)?;
        let message = if let Some(level) = level_up {
            format!("{message} · Уровень {level}!")
        } else {
            message
        };
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or("Save revision overflow")?;
        next.last_action_time = now;
        next.actions.push(Action {
            at_ms,
            shop: None,
            source: None,
            sequence: next.revision,
            at: now,
            uid: uid.into(),
            recipe: recipe_id,
            kind: kind.into(),
            gains,
            entropy: words,
            quest_events_pending: if next.systems.is_some() {
                Vec::new()
            } else {
                quest_events
            },
        });
        if let Some(s) = &mut next.systems {
            s.applied_action_revision = next.revision;
        }
        next.sync_preferences();
        next.validate(scene)?;
        Ok((next, message))
    }
    pub fn panel(&self, scene: &Scene, uid: &str, index: usize, wall: i64) -> (String, String) {
        let Some(config) = &scene.economy else {
            return ("Производство ещё не подключено".into(), String::new());
        };
        let Some(id) = self.choice(config, uid, index) else {
            return (
                "Выберите харвестер, нефтяную вышку или шахту".into(),
                String::new(),
            );
        };
        let Some(recipe) = config.recipes.get(&id) else {
            return (
                "Это производство сохранено; управление ещё не подключено".into(),
                String::new(),
            );
        };
        let e = &self.entities[uid];
        let outputs = recipe
            .outputs
            .iter()
            .map(|(k, v)| format!("{} {}", v.count, resource_name(k)))
            .collect::<Vec<_>>()
            .join(", ");
        if number(e, "productionType") != 0 {
            let remaining = number(e, "productionStartTime") + recipe.seconds
                - self.clock(config, wall).unwrap_or(0)
                + 1;
            if remaining > 0 {
                return (
                    format!(
                        "{outputs} · осталось {}:{:02}",
                        remaining / 60,
                        remaining % 60
                    ),
                    "Подождать".into(),
                );
            }
            return (
                format!(
                    "Готово: {outputs}, {} опыта · возможен предмет",
                    recipe.experience
                ),
                "Собрать".into(),
            );
        }
        let cost = if recipe.cost.is_empty() {
            "бесплатно".into()
        } else {
            recipe
                .cost
                .iter()
                .map(|(k, n)| format!("{n} {}", resource_name(k)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if self.needs_recipe_unlock(scene, id) {
            return (
                format!(
                    "{outputs} · нужен ур. {} · открыть раньше за {} кэша",
                    recipe.min_level,
                    recipe.unlock_price.unwrap_or(0)
                ),
                "Открыть рецепт".into(),
            );
        }
        (
            format!(
                "{outputs} · {} мин · {cost} · {}",
                recipe.seconds / 60,
                if self.resources["level"] < recipe.min_level {
                    "открыт заранее".into()
                } else {
                    format!("ур. {}", recipe.min_level)
                }
            ),
            "Начать".into(),
        )
    }
}
pub fn resource_name(key: &str) -> &str {
    match key {
        "playMoney" => "монет",
        "realMoney" => "кэша",
        "fuel" => "нефти",
        "iron" => "стали",
        "exp" => "опыта",
        k if k.starts_with("item:") => "предмет",
        _ => key,
    }
}
