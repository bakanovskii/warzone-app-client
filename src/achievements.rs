//! Original15achievement families/45tiers and once-only local warbond awards.
use crate::{
    Scene,
    production::{State, adjust},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Tier {
    pub need: i64,
    pub award: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Definition {
    pub id: u32,
    pub key: String,
    pub order: i64,
    pub tiers: Vec<Tier>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub definitions: Vec<Definition>,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        if self.definitions.len() != 15 {
            return Err("Incomplete original achievements catalog".into());
        }
        for d in &self.definitions {
            if d.id == 0
                || !ids.insert(d.id)
                || d.key.is_empty()
                || !keys.insert(&d.key)
                || d.tiers.len() != 3
                || d.tiers
                    .iter()
                    .any(|t| t.need <= 0 || t.award < 0 || t.award > 65535)
                || d.tiers.windows(2).any(|p| p[0].need >= p[1].need)
            {
                return Err("Invalid source achievement tiers".into());
            }
        }
        Ok(())
    }
    pub fn get(&self, id: u32) -> Option<&Definition> {
        self.definitions.iter().find(|d| d.id == id)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub count: i64,
    pub tier: usize,
    pub ready: bool,
    pub stars: usize,
    pub need: i64,
    pub award: i64,
}
impl State {
    /// Older published NPC saves missed these two source counters. Recover
    /// gold exactly from committed wallet receipts and only provable death
    /// minima (an existing death-drop entropy receipt). Never pay a reward,
    /// lower an imported counter, infer unrecorded field deaths or replay quests.
    pub fn initialize_achievement_counters(&mut self, scene: &Scene) -> Result<bool, String> {
        if scene.achievements.is_none() {
            return Ok(false);
        }
        let mut gold = 0i64;
        let mut deaths = 0i64;
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "pvp_loot" && a.recipe == 5990)
        {
            let n = *a.gains.get("playMoney").unwrap_or(&0);
            if n < 0 {
                return Err("Negative historical PvP income".into());
            }
            gold = gold
                .checked_add(n)
                .ok_or("Historical achievement overflow")?;
            deaths += i64::from(!a.entropy.is_empty());
        }
        let baseline = scene
            .progression
            .as_ref()
            .and_then(|p| p.seed_preferences.get("achievements"))
            .ok_or("Missing source achievement baseline")?;
        let systems = self.systems.as_mut().ok_or("Missing achievement state")?;
        let mut changed = false;
        for (key, delta) in [("steal_playmoney", gold), ("destroy_buildings", deaths)] {
            if delta == 0 {
                continue;
            }
            let base = baseline[key]["progress"]
                .as_i64()
                .ok_or("Invalid source achievement baseline")?;
            let expected = base
                .checked_add(delta)
                .ok_or("Historical achievement overflow")?;
            let raw = systems
                .preferences
                .get_mut("achievements")
                .and_then(|a| a.get_mut(key))
                .and_then(|a| a.as_object_mut())
                .ok_or("Missing historical achievement counter")?;
            let current = raw
                .get("progress")
                .and_then(|v| v.as_i64())
                .ok_or("Invalid historical achievement counter")?;
            if current < expected {
                raw.insert("progress".into(), expected.into());
                changed = true;
            }
        }
        Ok(changed)
    }
    pub fn achievement_progress(&self, d: &Definition) -> Progress {
        let raw = self
            .systems
            .as_ref()
            .and_then(|s| s.preferences.get("achievements"))
            .and_then(|a| a.get(&d.key));
        let count = raw.and_then(|v| v["progress"].as_i64()).unwrap_or(0);
        let received = |i: usize| {
            raw.and_then(|r| r["levels"][i]["awardReceived"].as_bool())
                .unwrap_or(false)
        };
        let tier = (0..d.tiers.len())
            .find(|&i| !received(i))
            .unwrap_or(d.tiers.len());
        if tier == d.tiers.len() {
            return Progress {
                count,
                tier: 4,
                ready: false,
                stars: 3,
                need: d.tiers[2].need,
                award: 0,
            };
        }
        let t = &d.tiers[tier];
        let ready = count >= t.need;
        Progress {
            count,
            tier: tier + 1,
            ready,
            stars: tier + usize::from(ready),
            need: t.need,
            award: t.award,
        }
    }
    pub fn achievement_awards_pending(&self, scene: &Scene) -> usize {
        scene.achievements.as_ref().map_or(0, |c| {
            c.definitions
                .iter()
                .map(|d| {
                    let raw = self
                        .systems
                        .as_ref()
                        .and_then(|s| s.preferences.get("achievements"))
                        .and_then(|a| a.get(&d.key));
                    let count = raw.and_then(|r| r["progress"].as_i64()).unwrap_or(0);
                    d.tiers
                        .iter()
                        .enumerate()
                        .filter(|(i, t)| {
                            count >= t.need
                                && !raw
                                    .and_then(|r| r["levels"][*i]["awardReceived"].as_bool())
                                    .unwrap_or(false)
                        })
                        .count()
                })
                .sum()
        })
    }
    pub fn claim_achievement(
        &self,
        scene: &Scene,
        id: u32,
        tier: usize,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let d = scene
            .achievements
            .as_ref()
            .and_then(|c| c.get(id))
            .ok_or("Достижение не найдено")?;
        let p = self.achievement_progress(d);
        if !p.ready || p.tier != tier {
            return Err("Награда ещё недоступна или уже получена".into());
        }
        if self
            .mission
            .as_ref()
            .is_some_and(|_| !self.mission_paused())
        {
            return Err("Награда доступна на базе".into());
        }
        let now = self.clock(
            scene.economy.as_ref().ok_or("Missing achievement clock")?,
            wall,
        )?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let data = next
            .systems
            .as_mut()
            .and_then(|s| s.preferences.get_mut("achievements"))
            .and_then(|a| a.get_mut(&d.key))
            .ok_or("Нет состояния достижения")?;
        let flag = data["levels"]
            .as_array_mut()
            .and_then(|a| a.get_mut(tier - 1))
            .and_then(|l| l.as_object_mut())
            .ok_or("Неверное состояние награды")?;
        flag.insert("awardReceived".into(), true.into());
        adjust(&mut next.resources, "realMoney", p.award)?;
        let systems = next.systems.as_mut().unwrap();
        let free = systems
            .integer("realMoneyFree")
            .checked_add(p.award)
            .filter(|n| *n < i32::MAX as i64)
            .ok_or("Achievement free cash overflow")?;
        systems.set("realMoneyFree".into(), free);
        next.building_receipt(
            now,
            &format!("achievement:{id}:{tier}"),
            id,
            "achievement_claim",
            BTreeMap::from([("realMoney".into(), p.award)]),
            vec![],
        )?;
        next.validate(scene)?;
        Ok((
            next,
            crate::interface::lang(scene, "#button_take_reward", "Награда получена").into(),
        ))
    }
    pub(crate) fn validate_achievement_claims(&self, scene: &Scene) -> Result<(), String> {
        let mut claimed = BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "achievement_claim")
        {
            let c = scene
                .achievements
                .as_ref()
                .ok_or("Achievement claims require the original catalog")?;
            let d = c.get(a.recipe).ok_or("Unknown claimed achievement")?;
            let prefix = format!("achievement:{}:", d.id);
            let tier = a
                .uid
                .strip_prefix(&prefix)
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|t| (1..=3).contains(t))
                .ok_or("Invalid achievement receipt")?;
            let raw = self
                .systems
                .as_ref()
                .and_then(|s| s.preferences.get("achievements"))
                .and_then(|a| a.get(&d.key))
                .ok_or("Missing claimed achievement state")?;
            if !claimed.insert((d.id, tier))
                || raw["progress"].as_i64().unwrap_or(0) < d.tiers[tier - 1].need
                || raw["levels"][tier - 1]["awardReceived"] != true
                || a.gains != BTreeMap::from([("realMoney".into(), d.tiers[tier - 1].award)])
                || !a.entropy.is_empty()
                || a.source.is_some()
                || a.shop.is_some()
            {
                return Err("Invalid or duplicate achievement payment".into());
            }
        }
        Ok(())
    }
}
