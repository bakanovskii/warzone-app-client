//! Original building purchases/timers/completion, independent of the guest CPU.
use crate::{
    Entity, Scene,
    entity_guid::entity_guid_from_words,
    production::{Action, State, adjust, number},
    systems::Award,
    world::{NE, NW, SE, SW},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

fn valid_direction(id: u32, direction: u32) -> bool {
    [SE, SW].contains(&direction)
        || ([1021, 1022, 1036].contains(&id) && [NE, NW].contains(&direction))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Limit {
    pub level: i64,
    pub count: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Bonus {
    pub kind: String,
    pub count: i64,
    pub probability: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Building {
    pub id: u32,
    pub order: i64,
    pub min_level: i64,
    pub cost: BTreeMap<String, i64>,
    pub limits: Vec<Limit>,
    pub times: BTreeMap<i64, i64>,
    pub awards: Vec<Bonus>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub buildings: BTreeMap<u32, Building>,
}
impl Building {
    pub fn max_count(&self, level: i64) -> i64 {
        if self.limits.is_empty() {
            return 65535;
        }
        self.limits
            .iter()
            .take_while(|v| v.level <= level)
            .last()
            .map_or(0, |v| v.count)
    }
    pub fn seconds(&self, existing: i64) -> i64 {
        self.times
            .range(..=existing + 1)
            .next_back()
            .map_or(0, |(_, v)| *v)
    }
}
pub fn phase(v: &serde_json::Value, now: i64) -> u32 {
    if number(v, "state") == 0 {
        return 0;
    }
    if number(v, "state") == 3 {
        return 3;
    }
    let total = number(v, "buildTotalTime");
    let elapsed = (now - number(v, "buildStartTime")) as f32;
    if total == 0 || elapsed >= total as f32 {
        3
    } else if (elapsed as f64) >= (total as f64) * 0.5 {
        2
    } else {
        1
    }
}
impl State {
    pub fn building_items<'a>(&self, scene: &'a Scene) -> Vec<&'a Building> {
        let mut items: Vec<_> = scene
            .construction
            .as_ref()
            .map(|c| c.buildings.values().collect())
            .unwrap_or_default();
        items.sort_by_key(|b| (b.order, b.id));
        items
    }
    pub fn building_count(&self, id: u32) -> i64 {
        self.entities
            .values()
            .filter(|v| v["type"] == id && v["isDeleted"] != true)
            .count() as i64
    }
    pub fn building_access(&self, b: &Building) -> i64 {
        let base = b.max_count(self.resources["level"]);
        let Some(s) = &self.systems else {
            return base;
        };
        if s.preferences.contains_key(&format!("TotalAccess-{}", b.id)) {
            return base.max(s.integer(&format!("TotalAccess-{}", b.id)));
        }
        base + s.integer(&format!("Access-{}", b.id)).max(0)
    }
    /// Source shop confirmation opens placement before NoResourcesDialog.
    /// Count/level/price validity still gate entry; affordability gates commit.
    pub fn building_placement_offer<'a>(
        &self,
        scene: &'a Scene,
        id: u32,
    ) -> Result<&'a Building, String> {
        let b = scene
            .construction
            .as_ref()
            .and_then(|c| c.buildings.get(&id))
            .ok_or("Это здание сейчас не продаётся")?;
        let count = self.building_count(id);
        if count >= b.max_count(35) || count >= self.building_access(b) {
            return Err("Достигнут лимит зданий: нужен следующий уровень".into());
        }
        if self.resources["level"] < b.min_level
            && self.building_access(b) <= b.max_count(self.resources["level"])
        {
            return Err(format!("Нужен уровень {}", b.min_level));
        }
        for (key, price) in &b.cost {
            if *price < 0 || !["playMoney", "realMoney", "fuel", "iron"].contains(&key.as_str()) {
                return Err("Invalid building price".into());
            }
        }
        Ok(b)
    }
    pub fn building_offer<'a>(&self, scene: &'a Scene, id: u32) -> Result<&'a Building, String> {
        let b = self.building_placement_offer(scene, id)?;
        if b.cost
            .iter()
            .any(|(key, price)| self.resources.get(key).copied().unwrap_or(0) < *price)
        {
            return Err("Недостаточно ресурсов для строительства".into());
        }
        Ok(b)
    }
    pub fn catalog_entity(
        &self,
        scene: &Scene,
        v: &serde_json::Value,
        now: Option<i64>,
    ) -> Result<Entity, String> {
        let id = number(v, "type") as u32;
        let d = scene
            .world
            .as_ref()
            .and_then(|c| c.catalog.get(&id))
            .ok_or("Unknown created entity type")?;
        let direction = number(v, "direction") as u32;
        let stage = if d.kind == "buildings" {
            phase(v, now.unwrap_or(number(v, "buildStartTime")))
        } else {
            3
        };
        let mut tracks = if stage == 3 {
            d.variants.get(&direction)
        } else {
            d.stages.get(&stage).and_then(|s| s.get(&direction))
        }
        .ok_or("Missing construction stage artwork")?
        .clone();
        if let Some(upgraded) = self.upgraded_tracks(scene, id, direction) {
            tracks = upgraded.clone();
        }
        if d.kind == "buildings" && number(v, "state") != 3 {
            tracks.retain(|t| t.fps == 0.0);
        }
        Ok(Entity {
            uid: v["uid"].as_str().ok_or("Missing created UID")?.into(),
            type_id: id,
            kind: d.kind.clone(),
            name: d.name.clone(),
            x: number(v, "x") as f64 - 16.0,
            y: number(v, "y") as f64 - 16.0,
            direction,
            width: d.width,
            height: d.height,
            archived: v.clone(),
            tracks,
        })
    }
    pub fn building_preview(
        &self,
        scene: &Scene,
        id: u32,
        x: i32,
        y: i32,
        direction: u32,
    ) -> Result<Entity, String> {
        self.catalog_entity(
            scene,
            &json!({"uid":"","type":id,"x":x+16,"y":y+16,"direction":direction,"state":3}),
            None,
        )
    }
    pub(crate) fn building_receipt(
        &mut self,
        now: i64,
        uid: &str,
        id: u32,
        kind: &str,
        gains: BTreeMap<String, i64>,
        entropy: Vec<u32>,
    ) -> Result<(), String> {
        self.revision = self.revision.checked_add(1).ok_or("Revision overflow")?;
        self.last_action_time = now;
        self.systems
            .as_mut()
            .ok_or("Missing native preferences")?
            .applied_action_revision = self.revision;
        self.actions.push(Action {
            at_ms: None,
            shop: None,
            source: None,
            sequence: self.revision,
            at: now,
            uid: uid.into(),
            recipe: id,
            kind: kind.into(),
            gains,
            entropy,
            quest_events_pending: vec![],
        });
        self.sync_preferences();
        Ok(())
    }
    pub fn buy_building(
        &self,
        scene: &Scene,
        id: u32,
        x: i32,
        y: i32,
        direction: u32,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        let b = self.building_offer(scene, id)?;
        if !valid_direction(id, direction) {
            return Err("Недопустимое направление нового здания".into());
        }
        self.placement_type(scene, id, "", x, y, direction)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let seconds = b.seconds(self.building_count(id));
        if seconds < 0 || now.checked_add(seconds).is_none_or(|v| v > i32::MAX as i64) {
            return Err("Недопустимый таймер строительства".into());
        }
        let mut words = [0; 6];
        for v in &mut words {
            *v = entropy()?;
        }
        let uid = entity_guid_from_words(words);
        if self.entities.contains_key(&uid) {
            return Err("Совпадение идентификаторов: покупка не выполнена".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        for (key, price) in &b.cost {
            adjust(&mut next.resources, key, -price)?;
            gains.insert(key.clone(), -price);
            if key == "realMoney" {
                let s = next.systems.as_mut().unwrap();
                s.set(
                    "realMoneyFree".into(),
                    (s.integer("realMoneyFree") - price).max(0),
                );
            }
        }
        let v = json!({"uid":uid,"type":id,"x":x+16,"y":y+16,"direction":direction,"state":if seconds==0 {3} else {1},
            "buildStep":0,"buildStartTime":now,"buildTotalTime":seconds,"productionType":0,"productionStartTime":0,
            "isDeleted":false,"isInBattle":false,"fraction":"player"});
        next.entities.insert(uid.clone(), v);
        next.selected = Some(uid.clone());
        next.building_receipt(now, &uid, id, "building_buy", gains, words.to_vec())?;
        next.validate(scene)?;
        Ok((next, "Здание размещено · строительство началось".into()))
    }
    pub fn construction_pending(&self, scene: &Scene, uid: &str) -> bool {
        self.entities.get(uid).is_some_and(|v| {
            v["isDeleted"] != true
                && [1, 2].contains(&number(v, "state"))
                && scene
                    .construction
                    .as_ref()
                    .is_some_and(|c| c.buildings.contains_key(&(number(v, "type") as u32)))
        })
    }
    pub fn finish_construction(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        if !self.construction_pending(scene, uid) {
            return Err("Строительство уже завершено".into());
        }
        let v = &self.entities[uid];
        let id = number(v, "type") as u32;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        if now < number(v, "buildStartTime") + number(v, "buildTotalTime") {
            return Err("Строительство ещё не закончено".into());
        }
        let b = &scene.construction.as_ref().unwrap().buildings[&id];
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        let mut words = vec![];
        next.entities.get_mut(uid).unwrap()["state"] = 3.into();
        // SceneMain.addNewItemTouch_: rand()%100 < chance, money THEN XP.
        // It first credits the bonus, then addFallingItem_ ->
        // addFallingItemOfType credits it a SECOND time. Confirmed against the
        // original ARM laboratory flow: +100coins/+2XP for XML50/1. Roll once,
        // preserve the source final state atomically; falling playback remains
        // a presentation concern. Historical receipts are never rewritten.
        let mut falling = Vec::new();
        for kind in ["playMoney", "experience"] {
            if kind == "experience" && next.at_top_experience(scene) {
                continue;
            }
            for bonus in b
                .awards
                .iter()
                .filter(|a| a.kind == kind && a.count != 0 && a.probability != 0)
            {
                let word = entropy()?;
                words.push(word);
                if (((word & 0x7fffffff) % 100) as i64) < bonus.probability {
                    next.award(
                        &Award {
                            kind: kind.into(),
                            count: bonus.count.to_string(),
                            item_id: 0,
                        },
                        scene,
                        now,
                        &mut gains,
                    )?;
                    falling.push((kind, bonus.count));
                }
            }
        }
        for (kind, count) in falling {
            if kind == "experience" && next.at_top_experience(scene) {
                continue;
            }
            next.award(
                &Award {
                    kind: kind.into(),
                    count: count.to_string(),
                    item_id: 0,
                },
                scene,
                now,
                &mut gains,
            )?;
        }
        if let Some(c) = &scene.progression {
            let level = next.resources["level"];
            let sys = next.systems.as_mut().unwrap();
            sys.event(c, level, 5, id)?;
            sys.event(c, level, 2, id)?;
        }
        next.update_level(scene, now, &mut gains)?;
        next.building_receipt(now, uid, id, "building_finish", gains, words)?;
        next.validate(scene)?;
        Ok((next, "Строительство завершено".into()))
    }
    pub fn validate_construction(&self, scene: &Scene) -> Result<(), String> {
        let seeds: HashSet<_> = scene.entities.iter().map(|e| e.uid.as_str()).collect();
        let c = scene
            .construction
            .as_ref()
            .ok_or("Missing construction rules")?;
        for (uid, v) in self
            .entities
            .iter()
            .filter(|(uid, _)| !seeds.contains(uid.as_str()))
        {
            let id = v["type"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or("Invalid new type")?;
            if self
                .actions
                .iter()
                .any(|a| a.kind == "shop_entity" && a.uid == *uid)
            {
                self.validate_shop_entity(scene, uid)?;
                continue;
            }
            if (2000..2999).contains(&id) && scene.workshop.is_some() {
                self.validate_trained_unit(scene, uid)?;
                continue;
            }
            let b = c.buildings.get(&id).ok_or("Unsupported new entity")?;
            if v["uid"] != uid.as_str()
                || !v["isDeleted"].is_boolean()
                || (v["isDeleted"] == true && scene.lifecycle.is_none())
                || v["isInBattle"] != false
                || v["fraction"] != "player"
                || ![1, 3].contains(&number(v, "state"))
                || number(v, "buildStep") != 0
            {
                return Err("Invalid created building state".into());
            }
            let receipt = self
                .actions
                .iter()
                .find(|a| a.kind == "building_buy" && &a.uid == uid && a.recipe == id)
                .ok_or("New entity has no purchase receipt")?;
            let words: [u32; 6] = receipt
                .entropy
                .clone()
                .try_into()
                .map_err(|_| "Invalid UID entropy receipt")?;
            if entity_guid_from_words(words) != *uid
                || self.forced_creation_start(uid, receipt.at) != number(v, "buildStartTime")
                || receipt.at <= 0
                || receipt.at > self.last_action_time
                || !(b.times.values().any(|t| *t == number(v, "buildTotalTime"))
                    || (b.times.is_empty() && number(v, "buildTotalTime") == 0))
            {
                return Err("Invalid creation identity/timer".into());
            }
            let finishes: Vec<_> = self
                .actions
                .iter()
                .filter(|a| a.kind == "building_finish" && &a.uid == uid)
                .collect();
            if number(v, "state") == 3 {
                if number(v, "buildTotalTime") > 0
                    && (finishes.len() != 1
                        || finishes[0].recipe != id
                        || finishes[0].at
                            < number(v, "buildStartTime") + number(v, "buildTotalTime")
                        || finishes[0].at > self.last_action_time)
                {
                    return Err("Completed building has no valid finish receipt".into());
                }
            } else if !finishes.is_empty()
                || number(v, "productionType") != 0
                || number(v, "productionStartTime") != 0
            {
                return Err("Unfinished building cannot restart or produce".into());
            }
            let pos = |k: &str| {
                v[k].as_i64()
                    .filter(|n| (0..128).contains(n))
                    .map(|n| n as i32)
                    .ok_or("Invalid created position")
            };
            let direction = v["direction"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or("Invalid created facing")?;
            if !valid_direction(id, direction) {
                return Err("Invalid created facing".into());
            }
            let (x, y) = (pos("x")? - 16, pos("y")? - 16);
            if v["isDeleted"] != true {
                self.placement_type(scene, id, uid, x, y, direction)?;
            }
            self.catalog_entity(scene, v, None)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shortfall {
    pub resources: BTreeMap<String, i64>,
    pub cash: i64,
}
impl State {
    pub fn building_shortfall(&self, scene: &Scene, id: u32) -> Result<Shortfall, String> {
        let b = self.building_placement_offer(scene, id)?;
        let pricing = scene
            .pricing
            .as_ref()
            .ok_or("Missing original resource prices")?;
        let mut resources = BTreeMap::new();
        let mut cash = 0i64;
        for key in ["playMoney", "fuel", "iron"] {
            let need = (b.cost.get(key).copied().unwrap_or(0) - self.resources[key]).max(0);
            if need > 0 {
                cash = cash
                    .checked_add(pricing.resource(key, need)?)
                    .ok_or("Resource price overflow")?;
                resources.insert(key.into(), need);
            }
        }
        Ok(Shortfall { resources, cash })
    }
    /// NoResourcesDialog: buy exactly the missing resources, then confirm the
    /// unchanged placement. Both receipts are committed atomically by caller.
    pub fn buy_building_with_shortfall(
        &self,
        scene: &Scene,
        id: u32,
        x: i32,
        y: i32,
        direction: u32,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        let q = self.building_shortfall(scene, id)?;
        if q.resources.is_empty() {
            return self.buy_building(scene, id, x, y, direction, wall, entropy);
        }
        self.placement_type(scene, id, "", x, y, direction)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        next.charge_cash(q.cash)?;
        let mut gains = q.resources.clone();
        gains.insert("realMoney".into(), -q.cash);
        for (key, amount) in &q.resources {
            adjust(&mut next.resources, key, *amount)?;
        }
        next.building_receipt(
            now,
            &format!("construction:{id}"),
            id,
            "building_resource_topup",
            gains,
            vec![],
        )?;
        next.buy_building(scene, id, x, y, direction, wall, entropy)
    }
    pub(crate) fn validate_building_topups(&self, scene: &Scene) -> Result<(), String> {
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "shop_resource_topup")
        {
            let item = scene
                .shop
                .as_ref()
                .and_then(|c| c.items.get(&a.recipe))
                .ok_or("Unknown shop topup")?;
            let p = scene.pricing.as_ref().ok_or("Missing resource pricing")?;
            let mut price = 0;
            if a.uid != format!("shop:{}", a.recipe) || !a.entropy.is_empty() || a.source.is_some()
            {
                return Err("Invalid shop topup identity".into());
            }
            for (k, v) in &a.gains {
                if k == "realMoney" {
                    continue;
                }
                if !["playMoney", "fuel", "iron"].contains(&k.as_str())
                    || *v <= 0
                    || *v > item.cost.get(k).copied().unwrap_or(0)
                {
                    return Err("Invalid shop topup quantity".into());
                }
                price += p.resource(k, *v)?;
            }
            if a.gains.len() < 2
                || a.gains.get("realMoney") != Some(&-price)
                || !self.actions.iter().any(|b| {
                    b.kind == "shop_entity"
                        && b.recipe == a.recipe
                        && b.at == a.at
                        && b.sequence == a.sequence + 1
                })
            {
                return Err("Unpaired shop topup".into());
            }
        }
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "building_resource_topup")
        {
            let b = scene
                .construction
                .as_ref()
                .and_then(|c| c.buildings.get(&a.recipe))
                .ok_or("Unknown topup building")?;
            let p = scene.pricing.as_ref().ok_or("Missing resource pricing")?;
            let mut price = 0;
            if a.uid != format!("construction:{}", a.recipe) || !a.entropy.is_empty() {
                return Err("Invalid construction topup identity".into());
            }
            for (k, v) in &a.gains {
                if k == "realMoney" {
                    continue;
                }
                if !["playMoney", "fuel", "iron"].contains(&k.as_str())
                    || *v <= 0
                    || *v > b.cost.get(k).copied().unwrap_or(0)
                {
                    return Err("Invalid construction topup quantity".into());
                }
                price += p.resource(k, *v)?;
            }
            if a.gains.len() < 2
                || a.gains.get("realMoney") != Some(&-price)
                || !self.actions.iter().any(|b| {
                    b.kind == "building_buy"
                        && b.recipe == a.recipe
                        && b.at == a.at
                        && b.sequence == a.sequence + 1
                })
            {
                return Err("Unpaired construction topup".into());
            }
        }
        Ok(())
    }
}
