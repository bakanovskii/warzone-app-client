//! Original Landscape/BuyEarthDialog purchase; no network or source-save writes.
use crate::{
    Scene,
    production::{State, adjust},
    world::expansion_position,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub prices: BTreeMap<u32, i64>,
    pub available: BTreeSet<u32>,
    pub fog: [i32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Payment {
    Coins,
    Cash,
    MissingCoins,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub coins: i64,
    pub cash: i64,
    pub missing: i64,
    pub topup: i64,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.prices.len() > 512
            || self.available.len() > 512
            || self.fog.iter().any(|n| !(-128..=256).contains(n))
            || self.fog[2] <= 0
            || self.fog[3] <= 0
        {
            return Err("Invalid expansion catalog".into());
        }
        for (id, cash) in &self.prices {
            expansion_position(*id as i32)?;
            if !(0..i32::MAX as i64).contains(cash) {
                return Err("Invalid expansion price".into());
            }
        }
        for id in &self.available {
            expansion_position(*id as i32)?;
        }
        Ok(())
    }
    fn candidate(&self, id: u32, owned: &Map<String, Value>) -> bool {
        let Ok([x, y]) = expansion_position(id as i32) else {
            return false;
        };
        let [fx, fy, fw, fh] = self.fog;
        (1..=15).contains(&x)
            && (1..=15).contains(&y)
            && self.available.contains(&id)
            && self.prices.contains_key(&id)
            && !owned.contains_key(&id.to_string())
            && (fx..fx + fw).contains(&(6 * x + 19))
            && (fy..fy + fh).contains(&(6 * y + 19))
            && owned
                .keys()
                .filter_map(|k| {
                    k.parse::<i32>()
                        .ok()
                        .and_then(|n| expansion_position(n).ok())
                })
                .any(|[a, b]| (a - x).abs() + (b - y).abs() == 1)
    }
}
impl State {
    fn owned_land(&self) -> Result<&Map<String, Value>, String> {
        self.systems
            .as_ref()
            .and_then(|s| s.preferences.get("expansionsMap"))
            .and_then(Value::as_object)
            .ok_or_else(|| "Missing owned land".into())
    }
    pub fn available_expansions(&self, scene: &Scene) -> Vec<u32> {
        let Some(c) = &scene.expansions else {
            return vec![];
        };
        let Ok(owned) = self.owned_land() else {
            return vec![];
        };
        c.prices
            .keys()
            .copied()
            .filter(|id| c.candidate(*id, owned))
            .collect()
    }
    pub fn expansion_at(
        &self,
        scene: &Scene,
        camera: &crate::Camera,
        point: [f64; 2],
    ) -> Option<u32> {
        let world = camera.world(point);
        let cell = scene.projection.inverse(world[0], world[1]);
        let tile = [(cell[0] / 6.).floor() as i32, (cell[1] / 6.).floor() as i32];
        self.available_expansions(scene)
            .into_iter()
            .find(|id| expansion_position(*id as i32).ok() == Some(tile))
    }
    pub fn expansion_quote(&self, scene: &Scene, id: u32) -> Result<Quote, String> {
        if self.mission.is_some() && !self.mission_paused() {
            return Err("Сначала вернись на базу".into());
        }
        let c = scene
            .expansions
            .as_ref()
            .ok_or("Missing expansion catalog")?;
        if !c.candidate(id, self.owned_land()?) {
            return Err("Этот участок недоступен".into());
        }
        let cash = *c
            .prices
            .get(&id)
            .ok_or("В исходнике нет цены этого участка")?;
        let p = scene.pricing.as_ref().ok_or("Missing original prices")?;
        let coins = p.from_cash("playMoney", cash)?;
        let missing = (coins - self.resources["playMoney"]).max(0);
        Ok(Quote {
            coins,
            cash,
            missing,
            topup: p.resource("playMoney", missing)?,
        })
    }
    pub fn buy_expansion(
        &self,
        scene: &Scene,
        id: u32,
        payment: Payment,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let quote = self.expansion_quote(scene, id)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        let kind = match payment {
            Payment::Coins => {
                adjust(&mut next.resources, "playMoney", -quote.coins)?;
                gains.insert("playMoney".into(), -quote.coins);
                "land_buy_coins"
            }
            Payment::Cash => {
                next.charge_cash(quote.cash)?;
                gains.insert("realMoney".into(), -quote.cash);
                "land_buy_cash"
            }
            Payment::MissingCoins => {
                if quote.missing == 0 {
                    return Err("Ресурсов уже достаточно; выбери покупку за монеты".into());
                }
                next.charge_cash(quote.topup)?;
                let used = self.resources["playMoney"];
                adjust(&mut next.resources, "playMoney", -used)?;
                gains.insert("playMoney".into(), -used);
                gains.insert("realMoney".into(), -quote.topup);
                "land_buy_shortfall"
            }
        };
        let systems = next.systems.as_mut().ok_or("Missing preferences")?;
        systems
            .preferences
            .get_mut("expansionsMap")
            .and_then(Value::as_object_mut)
            .ok_or("Missing land map")?
            .insert(id.to_string(), 1.into());
        systems.achievement("buy_expansions", 1, false);
        if let Some(c) = &scene.progression {
            systems.event(c, next.resources["level"], 6, 0)?;
        }
        next.building_receipt(now, &format!("expansion:{id}"), id, kind, gains, vec![])?;
        next.validate(scene)?;
        Ok((next, "Территория базы расширена".into()))
    }
    pub fn validate_expansions(&self, scene: &Scene) -> Result<(), String> {
        let Some(c) = &scene.expansions else {
            return Ok(());
        };
        let mut owned = scene
            .progression
            .as_ref()
            .and_then(|p| p.seed_preferences.get("expansionsMap"))
            .and_then(Value::as_object)
            .ok_or("Missing seed land")?
            .clone();
        let prices = scene.pricing.as_ref().ok_or("Missing prices")?;
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind.starts_with("land_buy_"))
        {
            let id = a.recipe;
            if !c.candidate(id, &owned)
                || a.uid != format!("expansion:{id}")
                || !a.entropy.is_empty()
                || a.shop.is_some()
                || a.source.is_some()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid land receipt".into());
            }
            let cash = *c.prices.get(&id).ok_or("Unpriced saved land")?;
            let coins = prices.from_cash("playMoney", cash)?;
            let expected = match a.kind.as_str() {
                "land_buy_cash" => BTreeMap::from([("realMoney".into(), -cash)]),
                "land_buy_coins" => BTreeMap::from([("playMoney".into(), -coins)]),
                "land_buy_shortfall" => {
                    let used = -a
                        .gains
                        .get("playMoney")
                        .copied()
                        .ok_or("Missing land coin debit")?;
                    if used < 0 || used >= coins {
                        return Err("Invalid land shortfall".into());
                    }
                    BTreeMap::from([
                        ("playMoney".into(), -used),
                        (
                            "realMoney".into(),
                            -prices.resource("playMoney", coins - used)?,
                        ),
                    ])
                }
                _ => return Err("Unknown land purchase".into()),
            };
            if a.gains != expected {
                return Err("Invalid land cost receipt".into());
            }
            owned.insert(id.to_string(), 1.into());
        }
        if self.owned_land()? != &owned {
            return Err("Owned land differs from seed and purchase history".into());
        }
        Ok(())
    }
}

/// SceneMain.isEarthOwned uses 6x6 cells at archived coordinate offset16.
pub fn owns_point(state: &State, x: i32, y: i32) -> bool {
    state.owned_land().is_ok_and(|owned| {
        owned
            .keys()
            .filter_map(|k| {
                k.parse::<i32>()
                    .ok()
                    .and_then(|n| expansion_position(n).ok())
            })
            .any(|[a, b]| (6 * a..6 * a + 6).contains(&x) && (6 * b..6 * b + 6).contains(&y))
    })
}
