//! Original SpeedCalc/ResPrice/XMLRecipes/SpecialItem prices. Currency is local
//! game state; these routines never contact a payment service or a server.
use crate::{
    Scene,
    production::{State, adjust},
    research::Research,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourcePrice {
    pub quantity: i64,
    pub price_real: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub resources: BTreeMap<String, ResourcePrice>,
    pub time_power_bits: u32,
    pub time_multiplier_bits: u32,
    pub time_subtrahend_bits: u32,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.time_power_bits != 0x3f000000
            || self.time_multiplier_bits != 0x43070000
            || self.time_subtrahend_bits != 0x41b20000
            || self.resources.len() != 3
            || ["playMoney", "fuel", "iron"]
                .iter()
                .any(|k| !self.resources.contains_key(*k))
            || self.resources.values().any(|p| {
                p.quantity <= 0
                    || p.quantity >= i32::MAX as i64
                    || p.price_real < 0
                    || p.price_real >= i32::MAX as i64
            })
        {
            return Err("Invalid original price configuration".into());
        }
        Ok(())
    }
    pub fn time(&self, seconds: i64) -> Result<i64, String> {
        if !(i32::MIN as i64..=i32::MAX as i64).contains(&seconds) {
            return Err("Price timer outside signed32 range".into());
        }
        // Thumb0x153de0: integer -> double division -> f32; each following
        // pow/multiply/subtract/ceil is binary32, without fused arithmetic.
        if (seconds as f64 / 60.) as f32 <= 5. {
            return Ok(0);
        }
        let base = (seconds as f64 / 3600.) as f32 / 3.;
        let power = base.powf(f32::from_bits(self.time_power_bits));
        let product = power * f32::from_bits(self.time_multiplier_bits);
        price_int((product - f32::from_bits(self.time_subtrahend_bits)).ceil())
    }
    pub fn from_cash(&self, kind: &str, cash: i64) -> Result<i64, String> {
        if !(0..i32::MAX as i64).contains(&cash) {
            return Err("Invalid cash conversion".into());
        }
        let p = self
            .resources
            .get(kind)
            .ok_or("Missing conversion resource")?;
        if p.price_real <= 0 {
            return Err("Invalid conversion denominator".into());
        }
        // ResPrice.calculateResource:fromRealMoney: preserves the f32 operations.
        price_int(((cash as f32 * p.quantity as f32) / p.price_real as f32).ceil())
    }
    pub fn resource(&self, kind: &str, count: i64) -> Result<i64, String> {
        if !(0..=i32::MAX as i64).contains(&count) {
            return Err("Resource price outside signed32 range".into());
        }
        let Some(p) = self.resources.get(kind) else {
            return Ok(1);
        }; // Original fallback.
        let value = count as f32 * (p.price_real as f32 / p.quantity as f32);
        price_int(if (value - value.floor()).abs() < 0.0001 {
            value.round()
        } else {
            value.ceil()
        })
    }
}
fn price_int(v: f32) -> Result<i64, String> {
    if !v.is_finite() || v < 0. || v as f64 >= i32::MAX as f64 {
        Err("Price overflow".into())
    } else {
        Ok(v as i64)
    }
}
fn add(total: &mut i64, unit: i64, count: i64) -> Result<(), String> {
    *total = total
        .checked_add(unit.checked_mul(count).ok_or("Price overflow")?)
        .filter(|n| *n >= 0 && *n < i32::MAX as i64)
        .ok_or("Price overflow")?;
    Ok(())
}
pub fn special_price(scene: &Scene, id: u32) -> Result<i64, String> {
    let c = scene.pricing.as_ref().ok_or("Расчёт цены недоступен")?;
    c.validate()?;
    special(scene, c, id, &mut BTreeSet::new())
}
fn special(
    scene: &Scene,
    c: &Config,
    id: u32,
    visiting: &mut BTreeSet<u32>,
) -> Result<i64, String> {
    let item = scene
        .progression
        .as_ref()
        .and_then(|s| s.special_items.get(&id))
        .ok_or("Материал не найден")?;
    if !(4800..=4820).contains(&id) || (4807..=4809).contains(&id) {
        return item
            .cost
            .get("realMoney")
            .copied()
            .filter(|n| *n >= 0 && *n < i32::MAX as i64)
            .ok_or_else(|| "Нет исходной цены предмета".into());
    }
    if visiting.len() >= 24 || !visiting.insert(id) {
        return Err("Cyclic material recipe".into());
    }
    let Some(r) = scene.workshop.as_ref().and_then(|s| {
        s.recipes
            .values()
            .find(|r| (2050..2071).contains(&r.id) && r.output == id)
    }) else {
        visiting.remove(&id);
        return Ok(0);
    };
    let mut total = c.time(r.seconds)?;
    for key in ["playMoney", "fuel", "iron"] {
        add(
            &mut total,
            c.resource(key, r.cost.get(key).copied().unwrap_or(0))?,
            1,
        )?;
    }
    for (item, count) in &r.items {
        if *count < 0 {
            return Err("Negative material input".into());
        }
        add(&mut total, special(scene, c, *item, visiting)?, *count)?;
    }
    visiting.remove(&id);
    Ok(total)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    pub cash: i64,
    pub consume: BTreeMap<String, i64>,
}
impl State {
    pub fn research_quote(&self, scene: &Scene, r: &Research) -> Result<Quote, String> {
        let c = scene.pricing.as_ref().ok_or("Расчёт цены недоступен")?;
        let mut total = c.time(r.seconds)?;
        let mut consume = BTreeMap::new();
        for (key, need) in &r.cost {
            if *need < 0 || !["playMoney", "fuel", "iron"].contains(&key.as_str()) {
                return Err("Invalid research currency".into());
            }
            let have = self.resources.get(key).copied().unwrap_or(0);
            if have < 0 {
                return Err("Negative resource balance".into());
            }
            let used = have.min(*need);
            add(&mut total, c.resource(key, need - used)?, 1)?;
            consume.insert(key.clone(), used);
        }
        for (id, need) in &r.items {
            if *need < 0 {
                return Err("Invalid research material count".into());
            }
            let have = self.inventory.get(&id.to_string()).copied().unwrap_or(0);
            if have < 0 {
                return Err("Negative inventory".into());
            }
            let used = have.min(*need);
            add(&mut total, special_price(scene, *id)?, need - used)?;
            consume.insert(format!("item:{id}"), used);
        }
        Ok(Quote {
            cash: total,
            consume,
        })
    }
    pub(crate) fn charge_cash(&mut self, cash: i64) -> Result<(), String> {
        if cash < 0 || cash >= i32::MAX as i64 {
            return Err("Invalid cash price".into());
        }
        if self.resources["realMoney"] < cash {
            return Err("Недостаточно банкнот".into());
        }
        adjust(&mut self.resources, "realMoney", -cash)?;
        let s = self.systems.as_mut().ok_or("Missing free cash state")?;
        s.set(
            "realMoneyFree".into(),
            (s.integer("realMoneyFree") - cash).max(0),
        );
        Ok(())
    }
    /// ResearchDialogUnit::buyItem buys ONE, not the whole missing quantity.
    pub fn buy_material(
        &self,
        scene: &Scene,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        if !(4801..=4809).contains(&id) {
            return Err("Этот материал не продаётся".into());
        }
        let cash = special_price(scene, id)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        next.charge_cash(cash)?;
        let gained = next.add_special_item(id, 1)?;
        let gains = BTreeMap::from([("realMoney".into(), -cash), (format!("item:{id}"), gained)]);
        next.building_receipt(
            now,
            &format!("inventory:{id}"),
            id,
            "material_buy",
            gains,
            vec![],
        )?;
        next.validate(scene)?;
        Ok((
            next,
            format!("Куплено: {}", scene.item_name(id, "Материал")),
        ))
    }
    pub fn validate_purchases(&self, scene: &Scene) -> Result<(), String> {
        self.validate_building_topups(scene)?;
        self.validate_production_topups(scene)?;
        self.validate_resource_speedups(scene)?;
        self.validate_production_speedups(scene)?;
        self.validate_construction_speedups(scene)?;
        for a in self.actions.iter().filter(|a| a.kind == "material_buy") {
            let price = special_price(scene, a.recipe)?;
            if a.uid != format!("inventory:{}", a.recipe)
                || a.gains
                    != BTreeMap::from([
                        ("realMoney".into(), -price),
                        (format!("item:{}", a.recipe), 1),
                    ])
                || !a.entropy.is_empty()
            {
                return Err("Invalid material purchase receipt".into());
            }
        }
        Ok(())
    }
}
