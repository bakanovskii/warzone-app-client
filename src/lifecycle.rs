//! SceneMain.onStartDelete/deleteItemConfirm and onBuildingRepair + 0x8fadc.
//! Deleted archives retain identity, position and production fields. No guest CPU.
use crate::{
    Scene,
    construction::Bonus,
    production::{State, adjust, number},
    systems::Award,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub sales: BTreeMap<u32, i64>,
    pub repairs: BTreeMap<u32, Bonus>,
    #[serde(default)]
    pub prices: BTreeMap<u32, i64>,
}
impl State {
    fn sale_value(&self, c: &Config, id: u32) -> Result<i64, String> {
        let nominal = c
            .sales
            .get(&id)
            .copied()
            .ok_or("Продажа этого типа пока недоступна")?;
        let price = if let Some(p) = c.prices.get(&id) {
            // UserProfile.getGameConstantFloat reads the profile dictionary only.
            // ObjC nil.floatValue is zero; constants.xml is NOT a runtime fallback.
            let v = self
                .systems
                .as_ref()
                .and_then(|s| s.preferences.get("constants"))
                .and_then(|v| v.get("SELL_COEF"));
            let coefficient = match v {
                None | Some(serde_json::Value::Null) => 0.0,
                Some(serde_json::Value::Number(n)) => {
                    n.as_f64().ok_or("Invalid sale coefficient")?
                }
                Some(serde_json::Value::String(s)) => {
                    s.parse::<f64>().map_err(|_| "Invalid sale coefficient")?
                }
                _ => return Err("Invalid sale coefficient".into()),
            } as f32;
            let value = (*p as f32) * coefficient;
            if !value.is_finite() || value < 0.0 || value >= i32::MAX as f32 {
                return Err("Sale amount outside supported range".into());
            }
            value as i64
        } else {
            nominal
        };
        if price < 0 {
            return Err("Invalid sale price".into());
        }
        Ok(price)
    }
    pub fn sale_quote(&self, scene: &Scene, uid: &str) -> Result<i64, String> {
        if self.gathering(uid) {
            return Err("Дождись завершения сбора".into());
        }
        let v = self.entities.get(uid).ok_or("Объект не найден")?;
        let id = number(v, "type") as u32;
        if v["isDeleted"] == true
            || v["isInBattle"] == true
            || [1000, 1026, 1027, 1028].contains(&id)
        {
            return Err("Этот объект нельзя продать".into());
        }
        self.sale_value(
            scene.lifecycle.as_ref().ok_or("Missing lifecycle rules")?,
            id,
        )
    }
    pub fn demolish(&self, scene: &Scene, uid: &str, wall: i64) -> Result<(Self, String), String> {
        let price = self.sale_quote(scene, uid)?;
        let v = &self.entities[uid];
        let id = number(v, "type") as u32;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        next.entities.get_mut(uid).unwrap()["isDeleted"] = true.into();
        let mut gains = BTreeMap::new();
        if price != 0 {
            adjust(&mut next.resources, "playMoney", price)?;
            gains.insert("playMoney".into(), price);
        }
        if next.selected.as_deref() == Some(uid) {
            next.selected = None;
        }
        next.building_receipt(now, uid, id, "entity_sell", gains, vec![])?;
        next.validate(scene)?;
        Ok((next, format!("Объект продан · +{price} монет")))
    }
    pub fn repair_pending(&self, scene: &Scene, uid: &str) -> bool {
        self.entities.get(uid).is_some_and(|v| {
            v["isDeleted"] != true
                && v["isInBattle"] != true
                && number(v, "state") == 0
                && scene
                    .lifecycle
                    .as_ref()
                    .is_some_and(|c| c.repairs.contains_key(&(number(v, "type") as u32)))
        })
    }
    pub fn repair_building(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        if !self.repair_pending(scene, uid) {
            return Err("Здание сейчас не требует ремонта".into());
        }
        let id = number(&self.entities[uid], "type") as u32;
        let bonus = &scene.lifecycle.as_ref().unwrap().repairs[&id];
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        next.entities.get_mut(uid).unwrap()["state"] = 3.into();
        let mut gains = BTreeMap::new();
        let mut words = vec![];
        // 0x8fc48..6a: unsigned arc4random modulo100, with NO sign mask.
        // The original delayed block grants XP; native save commits repair+XP together.
        if bonus.probability > 0 {
            let word = entropy()?;
            words.push(word);
            if ((word % 100) as i64) < bonus.probability {
                next.award(
                    &Award {
                        kind: "experience".into(),
                        count: bonus.count.to_string(),
                        item_id: 0,
                    },
                    scene,
                    now,
                    &mut gains,
                )?;
            }
        }
        next.update_level(scene, now, &mut gains)?;
        next.building_receipt(now, uid, id, "building_repair", gains, words)?;
        next.validate(scene)?;
        Ok((next, "Здание отремонтировано".into()))
    }
    pub fn validate_lifecycle(&self, scene: &Scene) -> Result<(), String> {
        let config = scene.lifecycle.as_ref().unwrap();
        for (uid, v) in &self.entities {
            let seed = scene.entities.iter().find(|s| &s.uid == uid);
            let id = number(v, "type") as u32;
            // Original Entities2.dat keeps tombstones. They predate the native
            // journal and must not require a fabricated sale receipt. Root
            // validation keeps each such archive exact and forbids resurrection.
            if seed.is_some_and(|e| e.archived["isDeleted"] == true) {
                continue;
            }
            if scene.obstacles.is_some() && seed.is_some_and(|e| e.kind == "obstacles") {
                continue;
            }
            let sales: Vec<_> = self
                .actions
                .iter()
                .filter(|a| a.uid == *uid && a.kind == "entity_sell")
                .collect();
            if v["isDeleted"] == true {
                if self
                    .actions
                    .iter()
                    .any(|a| a.uid == *uid && a.kind == "mission_casualty")
                {
                    if !(2000..3000).contains(&id)
                        || !sales.is_empty()
                        || v["isInBattle"] != false
                        || self.selected.as_ref() == Some(uid)
                    {
                        return Err("Invalid lost troop archive".into());
                    }
                    // Source/deployment provenance is checked by validate_mission.
                    continue;
                }
                let price = self.sale_value(config, id)?;
                if sales.len() != 1
                    || sales[0].recipe != id
                    || sales[0].at > self.last_action_time
                    || [1000, 1026, 1027, 1028].contains(&id)
                    || v["isInBattle"] == true
                    || !sales[0].entropy.is_empty()
                    || !sales[0].quest_events_pending.is_empty()
                    || sales[0].gains
                        != if price == 0 {
                            BTreeMap::new()
                        } else {
                            BTreeMap::from([("playMoney".into(), price)])
                        }
                    || self.selected.as_ref() == Some(uid)
                {
                    return Err("Invalid deletion receipt/state".into());
                }
                if self
                    .actions
                    .iter()
                    .any(|a| a.uid == *uid && a.sequence > sales[0].sequence)
                {
                    return Err("Deleted object has a later action".into());
                }
            } else if !sales.is_empty() {
                return Err("Sold object was resurrected".into());
            }
            let repairs: Vec<_> = self
                .actions
                .iter()
                .filter(|a| a.uid == *uid && a.kind == "building_repair")
                .collect();
            // Damage acquisition through combat is a future block. Accept repairs of
            // imported damaged buildings, without inventing a way to damage a base.
            if let Some(seed) = seed {
                if seed.kind == "buildings" && number(&seed.archived, "state") == 0 {
                    if number(v, "state") == 3 {
                        let b = config.repairs.get(&id).ok_or("Missing repair rule")?;
                        if repairs.len() != 1
                            || repairs[0].recipe != id
                            || repairs[0].at > self.last_action_time
                            || repairs[0].entropy.len() != usize::from(b.probability > 0)
                        {
                            return Err("Invalid repair receipt".into());
                        }
                    } else if number(v, "state") != 0 || !repairs.is_empty() {
                        return Err("Invalid damaged state".into());
                    }
                } else if !repairs.is_empty() {
                    return Err("Undamaged seed has a repair receipt".into());
                }
            } else if !repairs.is_empty() {
                return Err("Created damage lifecycle is not implemented".into());
            }
        }
        Ok(())
    }
}
