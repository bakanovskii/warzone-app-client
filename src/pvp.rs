//! Original offline PvP arena rules. No matchmaking or payment service calls.
//! SceneBattle0x58585/0x58779, Model0xecf99, UserProfile0x30f9d/0x316c9.
use crate::battle::Unit;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const RESOURCES: [&str; 3] = ["playMoney", "fuel", "iron"];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bonus {
    pub count: i32,
    pub probability: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LootConfig {
    pub weights: BTreeMap<u32, [f32; 3]>,
    pub bonuses: BTreeMap<u32, [Bonus; 3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    #[serde(rename = "iD")]
    pub id: i32,
    pub text: String,
    #[serde(default = "okay", rename = "btnText")]
    pub button: String,
    #[serde(default, rename = "actionID")]
    pub action: i32,
    #[serde(default, rename = "txtAction")]
    pub directive: String,
    #[serde(default, rename = "useArrow")]
    pub arrow: bool,
}
fn okay() -> String {
    "#OK".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Drop {
    pub kind: String,
    pub item_id: u32,
    pub count: i32,
    pub probability: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub steps: Vec<Step>,
    pub loot: LootConfig,
    #[serde(default)]
    pub destroy: BTreeMap<u32, Vec<Drop>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Loot {
    pub uid: String,
    pub contents: [i32; 3],
    pub stolen: [i32; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Arena {
    pub intro: usize,
    pub loot: Vec<Loot>,
    pub entropy: Vec<u32>,
    pub attackers: Vec<String>,
    pub messages: std::collections::VecDeque<String>,
    pub first_star_shown: bool,
    pub last_stars: u32,
    pub finish_hint_acknowledged: bool,
    pub headquarters_destroyed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub total: usize,
    pub destroyed: usize,
    pub destruction_finished: usize,
    pub headquarters: bool,
    pub stars: u32,
}
fn eligible(u: &Unit) -> bool {
    matches!(u.kind.as_str(), "building" | "decor")
        && !matches!(u.id, 3001 | 3002 | 3010 | 3011 | 1026 | 1028)
}
pub fn score(units: &[Unit], headquarters_already_destroyed: bool) -> Score {
    let mut s = Score {
        total: 0,
        destroyed: 0,
        destruction_finished: 0,
        headquarters: headquarters_already_destroyed,
        stars: 0,
    };
    for u in units.iter().filter(|u| eligible(u)) {
        s.total += 1;
        if u.hp <= 0 {
            s.destroyed += 1;
            s.destruction_finished += usize::from(u.damaged);
            s.headquarters |= u.id == 1000;
        }
    }
    // The source checks integer(trunc(float32(dead)*100/float32(total)))>29.
    // It also gives the all-buildings star for the empty arena; retain that
    // quirk here. A usable PvP arena is validated separately to contain an HQ.
    let percent = if s.total == 0 {
        i32::MIN
    } else {
        (s.destroyed as f32 * 100. / s.total as f32) as i32
    };
    s.stars =
        u32::from(percent > 29) + u32::from(s.destroyed == s.total) + u32::from(s.headquarters);
    s
}
pub fn match_price(level: i64) -> Result<i64, String> {
    if !(0..=i32::MAX as i64 / 10).contains(&level) {
        return Err("Invalid PvP level".into());
    }
    Ok(level * 10)
}
impl LootConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.weights.len() > 512
            || self.bonuses.len() > 512
            || self
                .weights
                .values()
                .flatten()
                .any(|x| !x.is_finite() || *x < 0. || *x > 1000000.)
            || self
                .bonuses
                .values()
                .flatten()
                .any(|b| b.count < 0 || b.count > 1000000 || b.probability > 100)
        {
            return Err("Invalid original PvP loot configuration".into());
        }
        Ok(())
    }
    /// Original distinct-type weights, divided equally among instances of that
    /// type. Dictionary iteration order is supplied by the arena, not invented
    /// from names or positions. Source coefficient defaults to1, not3.
    pub fn distribute(
        &self,
        units: &[Unit],
        resources: [i32; 3],
        coefficient: f32,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<Vec<Loot>, String> {
        self.validate()?;
        if units.len() > 4096
            || resources.iter().any(|x| *x < 0)
            || !coefficient.is_finite()
            || !(0.0..=10.).contains(&coefficient)
        {
            return Err("Invalid PvP resource allocation".into());
        }
        let mut counts = BTreeMap::<u32, usize>::new();
        for u in units.iter().filter(|u| (1000..2000).contains(&u.id)) {
            *counts.entry(u.id).or_default() += 1;
        }
        let mut totals = [0f32; 3];
        for id in counts.keys() {
            if let Some(w) = self.weights.get(id) {
                for r in 0..3 {
                    totals[r] += w[r];
                }
            }
        }
        let mut fractions = Vec::with_capacity(units.len());
        let mut result = Vec::with_capacity(units.len());
        let mut seen = BTreeSet::new();
        for u in units {
            if !seen.insert(&u.uid) {
                return Err("Duplicate PvP loot identity".into());
            }
            let mut f = [0.; 3];
            if let Some(w) = self.weights.get(&u.id) {
                for r in 0..3 {
                    if totals[r] > 0. {
                        f[r] = w[r] / (totals[r] * counts.get(&u.id).copied().unwrap_or(1) as f32);
                    }
                }
            }
            fractions.push(f);
            result.push(Loot {
                uid: u.uid.clone(),
                contents: [0; 3],
                stolen: [0; 3],
            });
        }
        for r in 0..3 {
            // Double .333333 literal, coefficient(float32) promoted to double;
            // only iron calls roundf after narrowing back to float32.
            let raw = coefficient as f64 * resources[r] as f64 * 0.333333;
            if raw > i32::MAX as f64 {
                return Err("PvP resource allocation overflow".into());
            }
            let pool = if r == 2 {
                (raw as f32).round() as i32
            } else {
                raw as i32
            };
            let mut remaining = pool;
            // Callback0x31b98 returns1for a lower fraction and0otherwise. Its
            // asymmetric Foundation ordering still needs cross-runtime verification;
            // keep the caller's source entity order rather than use an invalid Ord.
            for (i, u) in units.iter().enumerate() {
                let share = ((pool as f32 * fractions[i][r]).ceil() as i32).min(remaining);
                remaining -= share;
                if (1000..2000).contains(&u.id) && !matches!(u.id, 1026 | 1028) {
                    let word = entropy()? % 100;
                    let mut count = share;
                    if let Some(b) = self.bonuses.get(&u.id) {
                        if word < b[r].probability {
                            count = count.checked_add(b[r].count).ok_or("PvP loot overflow")?;
                        }
                    }
                    result[i].contents[r] = count;
                }
            }
        }
        Ok(result)
    }
}
impl Config {
    pub fn validate(&self, c: &crate::battle::Config) -> Result<(), String> {
        self.loot.validate()?;
        let m = c
            .missions
            .get("pvptutorial_1")
            .ok_or("Missing original5990arena")?;
        if m.entities.iter().any(|e| !c.defs.contains_key(&e.id))
            || m.entities
                .iter()
                .filter(|e| c.defs[&e.id].group == "units")
                .count()
                != 1
            || m.entities.iter().any(|e| {
                c.defs[&e.id].group == "units" && e.id != 2018
                    || c.defs[&e.id].group == "buildings" && c.defs[&e.id].attack > 0
            })
        {
            return Err("Only the recovered one-guard NPC arena is currently supported".into());
        }
        let ids = [
            10000, 10002, 10003, 10004, 10005, 10006, 10007, 10008, 10009, 10010, 10012, 10014,
            10015, 10016,
        ];
        if m.mission_id != 5990
            || self.steps.len() != ids.len()
            || self.steps.iter().zip(ids).any(|(s, id)| {
                s.id != id
                    || s.text.len() > 1000
                    || s.button.len() > 100
                    || s.action != 0 && s.action != 1
                    || s.action == 1
                        && !s
                            .directive
                            .strip_prefix("scroll:")
                            .is_some_and(|name| m.zones.iter().any(|z| z.name == name))
            })
            || self.destroy.len() > 512
            || self.destroy.values().any(|drops| {
                drops.len() > 32
                    || drops.iter().any(|d| {
                        d.count < 0
                            || d.count > 1000000
                            || d.probability > 100
                            || !matches!(d.kind.as_str(), "experience" | "item")
                    })
            })
        {
            return Err("Invalid original PvP source configuration".into());
        }
        Ok(())
    }
}
impl Arena {
    pub fn empty() -> Self {
        Self {
            intro: 0,
            loot: vec![],
            entropy: vec![],
            attackers: vec![],
            messages: Default::default(),
            first_star_shown: false,
            last_stars: 0,
            finish_hint_acknowledged: false,
            headquarters_destroyed: false,
        }
    }
    pub fn totals(&self) -> [i32; 3] {
        let mut out = [0; 3];
        for l in &self.loot {
            for (r, total) in out.iter_mut().enumerate() {
                *total += l.contents[r] + l.stolen[r];
            }
        }
        out
    }
    pub fn remaining(&self) -> [i32; 3] {
        let mut out = [0; 3];
        for l in &self.loot {
            for (r, total) in out.iter_mut().enumerate() {
                *total += l.contents[r];
            }
        }
        out
    }
    pub fn stolen(&self) -> [i32; 3] {
        let mut out = [0; 3];
        for l in &self.loot {
            for (r, total) in out.iter_mut().enumerate() {
                *total += l.stolen[r];
            }
        }
        out
    }
    /// battleCreateBonusesFrom0x55df9 uses CURRENT remaining resources and the
    /// post-shot health (not the previous health), then empties the building on
    /// its lethal hit. Damage/fraction arithmetic promotes to double in source.
    pub fn hit(
        &mut self,
        uid: &str,
        hp_after: i32,
        max: i32,
        damage: i32,
        has_destroy_bonus: bool,
    ) -> [i32; 3] {
        let mut out = [0; 3];
        let Some(l) = self.loot.iter_mut().find(|l| l.uid == uid) else {
            return out;
        };
        if max <= 0 || damage <= 0 {
            return out;
        }
        for (r, total) in out.iter_mut().enumerate() {
            let count = if hp_after <= 0 {
                if has_destroy_bonus { l.contents[r] } else { 0 }
            } else {
                ((damage.min(hp_after) as f64 * l.contents[r] as f64 * 0.5) / max as f64) as i32
            }
            .clamp(0, l.contents[r]);
            l.contents[r] -= count;
            l.stolen[r] += count;
            *total = count;
        }
        out
    }
}
