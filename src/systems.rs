//! Native progression recovered from Levels/UserProfile/Quests/QuestItem.
//! Preferences retain unknown fields; quests unlock from the original graph.
use crate::{
    Scene,
    production::{State, adjust, number},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Award {
    pub kind: String,
    pub count: String,
    pub item_id: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Level {
    pub id: i64,
    pub experience: i64,
    pub max_energy: i64,
    pub awards: Vec<Award>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Exercise {
    pub kind: String,
    pub value: i64,
    pub item_id: u32,
    pub recipes: Option<Vec<u32>>,
    pub text: String,
    pub icon: String,
    #[serde(default)]
    pub hint: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Quest {
    pub id: u32,
    pub level: i64,
    pub name: String,
    pub parents: Vec<u32>,
    pub exercises: Vec<Exercise>,
    pub awards: Vec<Award>,
    pub test_mode: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub levels: Vec<Level>,
    pub quests: Vec<Quest>,
    pub language: BTreeMap<String, String>,
    pub seed_preferences: BTreeMap<String, Value>,
    pub energy_recover_seconds: i64,
    #[serde(default)]
    pub special_items: BTreeMap<u32, SpecialItem>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpecialItem {
    pub id: u32,
    pub name: String,
    pub order: i32,
    pub cost: BTreeMap<String, i64>,
    pub outputs: Vec<Award>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Systems {
    pub preferences: BTreeMap<String, Value>,
    pub applied_action_revision: u64,
}
impl Systems {
    pub fn integer(&self, key: &str) -> i64 {
        self.preferences
            .get(key)
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }
    pub fn flag(&self, key: &str) -> bool {
        self.preferences
            .get(key)
            .is_some_and(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0))
    }
    pub fn set(&mut self, key: String, value: i64) {
        self.preferences.insert(key, value.into());
    }
    pub fn status(&self, id: u32) -> i64 {
        self.integer(&format!("QST-{id}"))
    }
    pub fn counter(&self, id: u32, step: usize) -> i64 {
        self.integer(&format!("Q{id}T{step}"))
    }
    pub fn parent_ready(&self, q: &Quest) -> bool {
        self.status(q.id) == 0 && q.parents.iter().all(|p| *p == 0 || self.status(*p) != 0)
    }
    pub fn active_ids(&self, config: &Config, level: i64) -> Vec<u32> {
        config
            .quests
            .iter()
            .filter(|q| self.parent_ready(q) && q.level <= level)
            .map(|q| q.id)
            .collect()
    }
    /// getActiveQuests filters the level; setQuestProgress uses isActive, which
    /// checks only parents/status. Keep these two original predicates distinct.
    pub fn event(
        &mut self,
        config: &Config,
        level: i64,
        kind: u32,
        item: u32,
    ) -> Result<(), String> {
        if kind == 6 {
            // Quests.checkQuests0x28169 stores getCurrentExpansions.count,
            // including imported land; it is not an increment from zero.
            let owned = self
                .preferences
                .get("expansionsMap")
                .and_then(Value::as_object)
                .ok_or("Missing quest expansion map")?
                .len() as i64;
            for q in &config.quests {
                if !self.parent_ready(q) || q.level > level {
                    continue;
                }
                for (i, e) in q.exercises.iter().enumerate() {
                    if e.kind == "buy_expansion" {
                        self.set(format!("Q{}T{i}", q.id), owned);
                    }
                }
            }
            return Ok(());
        }
        let event = match kind {
            1 => Some(("move_entities", 99)),
            2 | 5 => Some(("collect_entity", 1)),
            3 => Some(("start_recipe", 1)),
            4 => Some(("collect_recipe", 1)),
            7 => Some(("speed_up", 99)),
            8 => Some(("start_mission", 1)),
            9 => Some(("complete_mission", 1)),
            11 => Some(("rotate_entity", 99)),
            23 => Some(("collect_upgrade", 99)),
            30 => Some(("clear_junk", 99)),
            51 => Some(("start_research", 99)),
            52 => Some(("collect_research", 99)),
            81 => Some(("order_item_for_mission", 99)),
            _ => None,
        };
        let Some((name, value)) = event else {
            return Ok(());
        };
        for q in &config.quests {
            if !self.parent_ready(q) || q.level > level {
                continue;
            }
            for (i, e) in q.exercises.iter().enumerate() {
                if e.kind == name && (e.item_id == item || kind == 30) {
                    let count = if matches!(kind, 8 | 9) {
                        // Original SetExercise(value:1); reopening/restarting
                        // an unclaimed mission quest must not accumulate2,3,...
                        1
                    } else if value == 99 {
                        99
                    } else {
                        self.counter(q.id, i)
                            .checked_add(1)
                            .ok_or("Quest count overflow")?
                    };
                    self.set(format!("Q{}T{i}", q.id), count);
                }
            }
        }
        Ok(())
    }
    pub fn production_resource(
        &mut self,
        config: &Config,
        recipe: u32,
        resource: &str,
        count: i64,
    ) -> Result<(), String> {
        for q in &config.quests {
            if !self.parent_ready(q) {
                continue;
            }
            for (i, e) in q.exercises.iter().enumerate() {
                if e.recipes.as_ref().is_some_and(|ids| !ids.contains(&recipe)) {
                    continue;
                }
                // Original setQuestProgress:resType:recipe:pvp: checks resource
                // kind for money/fuel, but not for collect_iron (ARM0x2718e..271d0).
                let matches = match e.kind.as_str() {
                    "collect_playmoney" => resource == "playMoney",
                    "collect_fuel" => resource == "fuel",
                    "collect_iron" => true,
                    _ => false,
                };
                if matches {
                    let n = self
                        .counter(q.id, i)
                        .checked_add(count)
                        .ok_or("Quest count overflow")?;
                    self.set(format!("Q{}T{i}", q.id), n);
                }
            }
        }
        Ok(())
    }
    /// Original Quests.setQuestProgress:resType:recipe:pvp: keeps ordinary
    /// production counters separate. The source collect_pvp_iron branch has
    /// no resource-type guard; preserve that observable quirk as for iron.
    pub fn pvp_resource(
        &mut self,
        config: &Config,
        resource: &str,
        count: i64,
    ) -> Result<(), String> {
        if count < 0 || !matches!(resource, "playMoney" | "fuel" | "iron") {
            return Err("Invalid PvP quest resource event".into());
        }
        for q in &config.quests {
            if !self.parent_ready(q) {
                continue;
            }
            for (i, e) in q.exercises.iter().enumerate() {
                if e.recipes.as_ref().is_some_and(|r| !r.contains(&0)) {
                    continue;
                }
                if match e.kind.as_str() {
                    "collect_pvp_playmoney" => resource == "playMoney",
                    "collect_pvp_fuel" => resource == "fuel",
                    "collect_pvp_iron" => true,
                    _ => false,
                } {
                    self.set(
                        format!("Q{}T{i}", q.id),
                        self.counter(q.id, i)
                            .checked_add(count)
                            .ok_or("PvP quest count overflow")?,
                    );
                }
            }
        }
        Ok(())
    }
    pub fn achievement(&mut self, key: &str, delta: i64, absolute: bool) {
        let Some(ach) = self
            .preferences
            .get_mut("achievements")
            .and_then(Value::as_object_mut)
            .and_then(|a| a.get_mut(key))
        else {
            return;
        };
        let old = ach["progress"].as_i64().unwrap_or(0);
        if absolute {
            ach["progress"] = delta.into();
        } else if delta > 0 {
            ach["progress"] = old.saturating_add(delta).into();
        }
    }
}
impl State {
    /// Public imported identity, or a stable local base ID for sanitized legacy
    /// profiles. The protected HQ UID survives edits, payments, asset migrations
    /// and restart; hash it rather than displaying an exporter placeholder.
    /// serviceKey/sessionID are never used by the standalone client.
    pub fn local_player_id(&self) -> String {
        if let Some(id) = self
            .systems
            .as_ref()
            .and_then(|s| s.preferences.get("playerID"))
            .and_then(serde_json::Value::as_str)
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && !id.starts_with("LOCAL_TEST_")
                    && !id.starts_with("SOURCE_")
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            })
        {
            return id.to_owned();
        }
        self.entities
            .iter()
            .find(|(_, e)| e["type"] == 1000 && e["isDeleted"] != true)
            .map(|(uid, _)| format!("local-{}", &crate::save::digest(uid.as_bytes())[..8]))
            .unwrap_or_else(|| "local".into())
    }

    pub fn initialize_systems(&mut self, scene: &Scene) -> Result<bool, String> {
        let Some(config) = &scene.progression else {
            return Ok(false);
        };
        if self.systems.is_some() {
            return Ok(false);
        }
        let mut systems = Systems {
            preferences: config.seed_preferences.clone(),
            applied_action_revision: 0,
        };
        // Legacy production-only saves could never level up or claim quests.
        // Replay their journal into the unchanged imported quest graph once.
        for action in &mut self.actions {
            if !matches!(action.kind.as_str(), "start" | "collect") {
                return Err("Unsupported legacy journal migration".into());
            }
            for (kind, item) in &action.quest_events_pending {
                systems.event(config, self.resources["level"], *kind, *item)?;
            }
            if action.kind == "collect" {
                for key in ["playMoney", "fuel", "iron"] {
                    if let Some(n) = action.gains.get(key) {
                        systems.production_resource(config, action.recipe, key, *n)?;
                        let ach = match key {
                            "fuel" => "extract_oil",
                            "iron" => "extract_iron",
                            _ => "",
                        };
                        if !ach.is_empty() {
                            systems.achievement(ach, *n, false);
                        }
                    }
                }
                if action.gains.keys().any(|k| k.starts_with("item:")) {
                    systems.achievement("produce_items", 1, false);
                }
            }
            action.quest_events_pending.clear();
        }
        systems.applied_action_revision = self.revision;
        self.systems = Some(systems);
        self.sync_preferences();
        Ok(true)
    }
    pub fn sync_preferences(&mut self) {
        if let Some(s) = &mut self.systems {
            for (k, v) in &self.resources {
                s.preferences.insert(k.clone(), (*v).into());
            }
            s.preferences.insert(
                "specialItems".into(),
                serde_json::to_value(&self.inventory).unwrap(),
            );
        }
    }
    pub fn max_energy(&self, scene: &Scene) -> i64 {
        scene
            .progression
            .as_ref()
            .and_then(|c| c.levels.iter().find(|l| l.id == self.resources["level"]))
            .map_or(15, |l| l.max_energy)
    }
    pub fn energy(&self, scene: &Scene, now: i64) -> i64 {
        let Some(s) = &self.systems else { return 0 };
        // Exact signed32 arithmetic and division used by calculateEnergy.
        let recover = scene
            .progression
            .as_ref()
            .map_or(60, |c| c.energy_recover_seconds) as i32;
        let max = self.max_energy(scene) as i32;
        let scaled = max.wrapping_mul(recover);
        let amount = scaled
            .wrapping_sub(s.integer("energyTime") as i32)
            .wrapping_add(now as i32);
        if amount < scaled {
            (amount / recover) as i64
        } else {
            max as i64
        }
    }
    pub fn energy_next_seconds(&self, scene: &Scene, now: i64) -> i64 {
        let recover = scene
            .progression
            .as_ref()
            .map_or(60, |c| c.energy_recover_seconds);
        let Some(s) = &self.systems else {
            return recover;
        };
        let scaled = (self.max_energy(scene) as i32).wrapping_mul(recover as i32);
        let value = scaled
            .wrapping_sub(s.integer("energyTime") as i32)
            .wrapping_add(now as i32);
        if value < scaled {
            (recover - value as i64 % recover).max(0)
        } else {
            recover
        }
    }
    pub fn spend_energy(&mut self, scene: &Scene, now: i64, count: i64) -> Result<(), String> {
        if count < 0 || count > self.energy(scene, now) {
            return Err("Недостаточно энергии".into());
        }
        let recover = scene
            .progression
            .as_ref()
            .map_or(60, |c| c.energy_recover_seconds);
        let full = self.energy(scene, now) >= self.max_energy(scene);
        let s = self.systems.as_mut().ok_or("Energy state is unavailable")?;
        if count > 0 {
            let base = if full { now } else { s.integer("energyTime") };
            let end = base
                .checked_add(count.checked_mul(recover).ok_or("Energy overflow")?)
                .ok_or("Energy overflow")?;
            if end > i32::MAX as i64 {
                return Err("Energy timer overflow".into());
            }
            s.set("energyTime".into(), end);
        }
        Ok(())
    }
    pub fn at_top_experience(&self, scene: &Scene) -> bool {
        scene
            .progression
            .as_ref()
            .and_then(|c| c.levels.iter().map(|l| l.experience).max())
            .is_some_and(|top| self.resources["exp"] >= top)
    }
    pub(crate) fn award(
        &mut self,
        award: &Award,
        scene: &Scene,
        now: i64,
        gains: &mut BTreeMap<String, i64>,
    ) -> Result<(), String> {
        if award.kind == "energy" && award.count == "max" {
            self.systems
                .as_mut()
                .ok_or("Missing energy state")?
                .set("energyTime".into(), now - 1);
            return Ok(());
        }
        let count = award
            .count
            .parse::<i64>()
            .map_err(|_| "Unsupported award count")?;
        if count < 0 {
            return Err("Negative award".into());
        }
        let key = match award.kind.as_str() {
            "playMoney" | "fuel" | "iron" => award.kind.clone(),
            "realMoney" => {
                let s = self.systems.as_mut().ok_or("Missing free cash state")?;
                let free = s
                    .integer("realMoneyFree")
                    .checked_add(count)
                    .filter(|n| *n < i32::MAX as i64)
                    .ok_or("Free cash overflow")?;
                s.set("realMoneyFree".into(), free);
                "realMoney".into()
            }
            "experience" => {
                if self.at_top_experience(scene) {
                    return Ok(());
                }
                "exp".into()
            }
            "item" => {
                let actual = self.add_special_item(award.item_id, count)?;
                let key = format!("item:{}", award.item_id);
                *gains.entry(key).or_default() += actual;
                return Ok(());
            }
            _ => return Err(format!("Unsupported original award {}", award.kind)),
        };
        adjust(&mut self.resources, &key, count)?;
        *gains.entry(key).or_default() += count;
        Ok(())
    }
    pub fn update_level(
        &mut self,
        scene: &Scene,
        now: i64,
        gains: &mut BTreeMap<String, i64>,
    ) -> Result<Option<i64>, String> {
        let Some(config) = &scene.progression else {
            return Ok(None);
        };
        // getMaxLevel scans the original table backwards. Only destination
        // level awards are issued, even when several levels were skipped.
        let target = config
            .levels
            .iter()
            .rev()
            .find(|l| l.experience <= self.resources["exp"])
            .map(|l| l.id)
            .unwrap_or(1);
        if target <= self.resources["level"] {
            return Ok(None);
        }
        self.resources.insert("level".into(), target);
        if let Some(s) = &mut self.systems {
            s.achievement("get_level", target, true);
        }
        let level = config
            .levels
            .iter()
            .find(|l| l.id == target)
            .ok_or("Missing level definition")?;
        for award in &level.awards {
            self.award(award, scene, now, gains)?;
        }
        Ok(Some(target))
    }
    pub fn production_progress(
        &mut self,
        scene: &Scene,
        kind: &str,
        recipe: u32,
        level_before: i64,
        rewards: &BTreeMap<String, i64>,
    ) -> Result<(), String> {
        let Some(config) = &scene.progression else {
            return Ok(());
        };
        let s = self.systems.as_mut().ok_or("Missing native progression")?;
        if kind == "start" {
            s.event(config, level_before, 3, recipe)?;
        }
        if matches!(kind, "collect" | "gather_collect") {
            if kind == "collect" {
                s.event(config, level_before, 4, recipe)?;
            }
            for key in ["playMoney", "fuel", "iron"] {
                if let Some(n) = rewards.get(key) {
                    s.production_resource(config, recipe, key, *n)?;
                    let ach = match key {
                        "fuel" => "extract_oil",
                        "iron" => "extract_iron",
                        _ => "",
                    };
                    if !ach.is_empty() {
                        s.achievement(ach, *n, false);
                    }
                }
            }
            if rewards.keys().any(|k| k.starts_with("item:")) {
                s.achievement("produce_items", 1, false);
            }
        }
        Ok(())
    }
    pub fn active_quests<'a>(&self, scene: &'a Scene) -> Vec<&'a Quest> {
        let (Some(c), Some(s)) = (&scene.progression, &self.systems) else {
            return vec![];
        };
        c.quests
            .iter()
            .filter(|q| s.parent_ready(q) && q.level <= self.resources["level"])
            .collect()
    }
    pub fn exercise_progress(&self, scene: &Scene, q: &Quest, i: usize) -> (i64, bool) {
        let Some(s) = &self.systems else {
            return (0, false);
        };
        let e = &q.exercises[i];
        let stored = s.counter(q.id, i);
        if stored == 99 {
            return (e.value, true);
        }
        let count = match e.kind.as_str() {
            "collect_entity" => self
                .entities
                .values()
                .filter(|v| {
                    v["type"] == e.item_id
                        && v["isDeleted"] != true
                        && (!(1000..2000).contains(&e.item_id) || number(v, "state") == 3)
                })
                .count() as i64,
            "buy_expansion" => s
                .preferences
                .get("expansionsMap")
                .and_then(Value::as_object)
                .map_or(0, |v| v.len() as i64),
            "clear_junk" => {
                if !scene.entities.iter().any(|v| {
                    v.kind == "obstacles"
                        && self.entities[&v.uid]["isDeleted"] != true
                        && crate::expansions::owns_point(self, v.x as i32, v.y as i32)
                }) {
                    e.value
                } else {
                    stored
                }
            }
            "start_research" | "collect_research"
                if e.item_id == 0 || s.flag(&format!("Research-{}", e.item_id)) =>
            {
                e.value
            }
            "collect_upgrade" if s.flag(&format!("Upgrade-{}", e.item_id)) => e.value,
            "start_mission" | "complete_mission" if s.flag(&format!("Mission-{}", e.item_id)) => {
                e.value
            }
            "order_item_for_mission"
                if s.flag(&format!("Mission-{}", e.item_id))
                    || s.flag(&format!("MissionStarted-{}", e.item_id)) =>
            {
                e.value
            }
            _ => stored,
        };
        (count, count >= e.value)
    }
    pub fn quest_ready(&self, scene: &Scene, q: &Quest) -> bool {
        q.exercises
            .iter()
            .enumerate()
            .all(|(i, _)| self.exercise_progress(scene, q, i).1)
    }
    pub fn claim_quest(&self, scene: &Scene, id: u32, wall: i64) -> Result<(Self, String), String> {
        let config = scene
            .progression
            .as_ref()
            .ok_or("Quest definitions unavailable")?;
        let q = config
            .quests
            .iter()
            .find(|q| q.id == id)
            .ok_or("Задание не найдено")?;
        if !self.active_quests(scene).iter().any(|q| q.id == id) {
            return Err("Задание ещё недоступно или награда уже получена".into());
        }
        if !self.quest_ready(scene, q) {
            return Err("Сначала выполните все условия задания".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock config")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        next.systems
            .as_mut()
            .ok_or("Missing quest state")?
            .set(format!("QST-{id}"), 99);
        for a in &q.awards {
            next.award(a, scene, now, &mut gains)?;
        }
        next.systems
            .as_mut()
            .unwrap()
            .achievement("complete_quests", 1, false);
        let level = next.update_level(scene, now, &mut gains)?;
        next.revision = next.revision.checked_add(1).ok_or("Revision overflow")?;
        next.last_action_time = now;
        next.systems.as_mut().unwrap().applied_action_revision = next.revision;
        next.actions.push(crate::production::Action {
            at_ms: None,
            shop: None,
            source: None,
            sequence: next.revision,
            at: now,
            uid: format!("quest:{id}"),
            recipe: id,
            kind: "quest_claim".into(),
            gains,
            entropy: vec![],
            quest_events_pending: vec![],
        });
        next.sync_preferences();
        next.validate(scene)?;
        let suffix = level.map_or(String::new(), |l| format!(" · Уровень {l}!"));
        Ok((next, format!("Задание «{}» выполнено{suffix}", q.name)))
    }
}
