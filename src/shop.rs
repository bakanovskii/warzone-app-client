//! Original ShopDialog catalog, count/level/research gates and access price curve.
use crate::{Scene, construction::Limit, production::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Point {
    pub number: i64,
    pub price: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Item {
    pub id: u32,
    pub kind: String,
    pub tab: u32,
    pub order: i64,
    pub name: String,
    pub thumbnail: String,
    pub min_level: i64,
    pub limits: Vec<Limit>,
    pub research: u32,
    pub cost: BTreeMap<String, i64>,
    pub unlock_price: i64,
    pub unlock_points: Vec<Point>,
    pub stats: BTreeMap<String, i64>,
    pub ability: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub items: BTreeMap<u32, Item>,
    pub tab_modes: Vec<u32>,
}
impl Item {
    pub fn max_count(&self, level: i64) -> i64 {
        if self.limits.is_empty() {
            65535
        } else {
            self.limits
                .iter()
                .take_while(|p| p.level <= level)
                .last()
                .map_or(0, |p| p.count)
        }
    }
    pub fn unlock_price(&self, owned: i64) -> Result<i64, String> {
        if !(0..65535).contains(&owned) {
            return Err("Invalid source shop count".into());
        }
        let points = &self.unlock_points;
        if points.is_empty() {
            return Ok(self.unlock_price);
        }
        // Original one-element branch asks for the absent `count` key, hence0.
        if points.len() == 1 {
            return Ok(0);
        }
        let wanted = owned + 1;
        for p in points.windows(2) {
            let (a, b) = (&p[0], &p[1]);
            if a.number <= wanted && wanted < b.number {
                return (wanted - a.number)
                    .checked_mul(b.price - a.price)
                    .and_then(|v| v.checked_div(b.number - a.number))
                    .and_then(|v| v.checked_add(a.price))
                    .ok_or_else(|| "Shop access price overflow".into());
            }
        }
        Ok(points
            .iter()
            .skip(1)
            .max_by_key(|p| p.number)
            .map_or(0, |p| p.price))
    }
    pub fn next_level(&self, owned: i64) -> Option<i64> {
        self.limits
            .iter()
            .find(|p| p.count > owned)
            .map(|p| p.level)
    }
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.tab_modes != [1, 2, 3, 4, 5, 6, 7] || self.items.len() > 256 {
            return Err("Invalid source shop tabs/catalog".into());
        }
        for (&id, i) in &self.items {
            if id != i.id
                || !(2..=7).contains(&i.tab)
                || !scene.textures.contains_key(&i.thumbnail)
                || !["buildings", "decor", "units", "special_items"].contains(&i.kind.as_str())
                || !(0..=35).contains(&i.min_level)
                || i.unlock_price < 0
                || i.unlock_points.len() > 32
                || i.cost.iter().any(|(k, v)| {
                    !["playMoney", "realMoney", "fuel", "iron"].contains(&k.as_str())
                        || *v < 0
                        || *v >= i32::MAX as i64
                })
                || i.limits
                    .iter()
                    .any(|p| p.level < 0 || p.count < 0 || p.count > 65535)
                || i.unlock_points
                    .iter()
                    .any(|p| p.number < 1 || p.price < 0 || p.price >= i32::MAX as i64)
                || i.unlock_points
                    .windows(2)
                    .any(|p| p[0].number >= p[1].number)
                || (i.kind != "special_items"
                    && !scene
                        .world
                        .as_ref()
                        .is_some_and(|w| w.catalog.contains_key(&id)))
            {
                return Err(format!("Invalid original shop item {id}"));
            }
        }
        Ok(())
    }
}
impl State {
    pub fn shop_access(&self, item: &Item) -> i64 {
        let base = item.max_count(self.resources["level"]);
        self.systems.as_ref().map_or(base, |s| {
            if s.preferences
                .contains_key(&format!("TotalAccess-{}", item.id))
            {
                base.max(s.integer(&format!("TotalAccess-{}", item.id)))
            } else {
                base + s.integer(&format!("Access-{}", item.id)).max(0)
            }
        })
    }
    pub fn shop_locked(&self, item: &Item) -> bool {
        if item.kind == "special_items" {
            return false;
        }
        let owned = self.building_count(item.id);
        let access = self.shop_access(item);
        owned >= access.min(item.max_count(35))
            || (self.resources["level"] < item.min_level
                && (access == 0
                    || owned >= access
                    || (item.limits.is_empty()
                        && access <= item.max_count(self.resources["level"]))))
            || (item.research != 0
                && !self
                    .systems
                    .as_ref()
                    .is_some_and(|s| s.flag(&format!("Research-{}", item.research))))
    }
    pub fn source_shop_items<'a>(&self, scene: &'a Scene, tab: u32) -> Vec<&'a Item> {
        let mut items: Vec<_> = scene
            .shop
            .as_ref()
            .map(|c| c.items.values().filter(|i| i.tab == tab).collect())
            .unwrap_or_default();
        // The source block at0xdc284 returns only0/1 (no negative result), so
        // actual ARM ordering depends on NSDictionary enumeration. Use stable
        // ID ties for the same top-quantity/list-order priorities, never prices.
        items.sort_by_key(|i| {
            let top = self.building_count(i.id) >= i.max_count(35);
            (
                top,
                if top || i.order == 0 {
                    i.id as i64
                } else {
                    i.order
                },
                i.id,
            )
        });
        items
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccessReceipt {
    pub owned: i64,
    pub previous: i64,
    pub granted: i64,
    pub level: i64,
}
impl State {
    pub fn shop_placement_offer<'a>(&self, scene: &'a Scene, id: u32) -> Result<&'a Item, String> {
        let item = scene
            .shop
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .ok_or("Товар не найден")?;
        if !["decor", "units"].contains(&item.kind.as_str()) {
            return Err("Этот товар не размещается как готовый объект".into());
        }
        if self.shop_locked(item) {
            return Err("Для покупки нужен доступ".into());
        }
        // Shop mercenaries bypass the training population gate. Resource
        // payment belongs to placement confirmation, including fences.
        Ok(item)
    }
    pub fn unlock_shop_item(
        &self,
        scene: &Scene,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let item = scene
            .shop
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .ok_or("Товар не найден")?;
        let owned = self.building_count(id);
        let previous = self.shop_access(item);
        if !self.shop_locked(item)
            || owned >= item.max_count(35)
            || (owned < previous && self.resources["level"] >= item.min_level)
        {
            return Err("Открытие доступа сейчас не требуется".into());
        }
        let price = item.unlock_price(owned)?;
        if price <= 0 {
            return Err("Доступ нельзя купить".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        next.charge_cash(price)?;
        let granted = previous
            .checked_add(if [3001, 3002, 3010, 3011].contains(&id) {
                10
            } else {
                1
            })
            .ok_or("Shop access overflow")?;
        next.systems
            .as_mut()
            .ok_or("Missing access preferences")?
            .set(format!("TotalAccess-{id}"), granted);
        next.building_receipt(
            now,
            &format!("shop:{id}"),
            id,
            "shop_access",
            BTreeMap::from([("realMoney".into(), -price)]),
            vec![],
        )?;
        next.actions.last_mut().unwrap().shop = Some(AccessReceipt {
            owned,
            previous,
            granted,
            level: self.resources["level"],
        });
        next.validate(scene)?;
        Ok((next, "Доступ открыт".into()))
    }
    pub fn buy_shop_entity(
        &self,
        scene: &Scene,
        id: u32,
        x: i32,
        y: i32,
        direction: u32,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        use crate::{
            entity_guid::entity_guid_from_words,
            production::adjust,
            world::{NE, NW, SE, SW},
        };
        let item = self.shop_placement_offer(scene, id)?;
        for (key, price) in &item.cost {
            if self.resources.get(key).copied().unwrap_or(0) < *price {
                return Err("Недостаточно ресурсов для покупки".into());
            }
        }
        if ![SE, SW, NE, NW].contains(&direction) {
            return Err("Недопустимое направление объекта".into());
        }
        self.placement_type(scene, id, "", x, y, direction)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut words = [0; 6];
        for word in &mut words {
            *word = entropy()?
        }
        let uid = entity_guid_from_words(words);
        if self.entities.contains_key(&uid) {
            return Err("Совпадение идентификаторов: покупка не выполнена".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        for (key, price) in &item.cost {
            if key == "realMoney" {
                next.charge_cash(*price)?
            } else {
                adjust(&mut next.resources, key, -price)?
            }
            gains.insert(key.clone(), -price);
        }
        next.entities.insert(uid.clone(),serde_json::json!({"uid":uid,"type":id,"x":x+16,"y":y+16,"direction":direction,"state":0,"buildStep":0,"buildStartTime":0,"buildTotalTime":0,"productionType":0,"productionStartTime":0,"isDeleted":false,"isInBattle":false,"fraction":"player"}));
        if item.kind == "units" {
            let s = next.systems.as_mut().ok_or("Missing quest state")?;
            s.event(
                scene.progression.as_ref().ok_or("Missing quests")?,
                self.resources["level"],
                5,
                id,
            )?;
            s.set("firstUnitPlacing".into(), 0);
        }
        next.selected = Some(uid.clone());
        next.building_receipt(now, &uid, id, "shop_entity", gains, words.to_vec())?;
        next.validate(scene)?;
        Ok((next, "Объект размещён".into()))
    }
    pub fn shop_entity_shortfall(
        &self,
        scene: &Scene,
        id: u32,
    ) -> Result<crate::construction::Shortfall, String> {
        let item = self.shop_placement_offer(scene, id)?;
        let pricing = scene
            .pricing
            .as_ref()
            .ok_or("Missing original resource prices")?;
        let mut resources = BTreeMap::new();
        let mut cash = 0i64;
        for key in ["playMoney", "fuel", "iron"] {
            let need = (item.cost.get(key).copied().unwrap_or(0) - self.resources[key]).max(0);
            if need > 0 {
                cash = cash
                    .checked_add(pricing.resource(key, need)?)
                    .ok_or("Resource price overflow")?;
                resources.insert(key.into(), need);
            }
        }
        Ok(crate::construction::Shortfall { resources, cash })
    }
    pub fn buy_shop_entity_with_shortfall(
        &self,
        scene: &Scene,
        id: u32,
        x: i32,
        y: i32,
        direction: u32,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        let q = self.shop_entity_shortfall(scene, id)?;
        if q.resources.is_empty() {
            return self.buy_shop_entity(scene, id, x, y, direction, wall, entropy);
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
        for (k, n) in &q.resources {
            crate::production::adjust(&mut next.resources, k, *n)?;
        }
        next.building_receipt(
            now,
            &format!("shop:{id}"),
            id,
            "shop_resource_topup",
            gains,
            vec![],
        )?;
        next.buy_shop_entity(scene, id, x, y, direction, wall, entropy)
    }
    pub(crate) fn validate_shop_entity(&self, scene: &Scene, uid: &str) -> Result<(), String> {
        use crate::{
            entity_guid::entity_guid_from_words,
            production::number,
            world::{NE, NW, SE, SW},
        };
        let e = &self.entities[uid];
        let id = number(e, "type") as u32;
        let item = scene
            .shop
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .filter(|i| ["decor", "units"].contains(&i.kind.as_str()))
            .ok_or("Unknown shop entity")?;
        let receipts: Vec<_> = self
            .actions
            .iter()
            .filter(|a| a.uid == uid && a.kind == "shop_entity")
            .collect();
        if receipts.len() != 1 {
            return Err("Shop entity has no unique purchase".into());
        }
        let a = receipts[0];
        let cost: BTreeMap<_, _> = item.cost.iter().map(|(k, v)| (k.clone(), -*v)).collect();
        if a.recipe != id
            || a.at <= 0
            || a.at > self.last_action_time
            || a.source.is_some()
            || a.shop.is_some()
            || a.entropy.len() != 6
            || entity_guid_from_words(a.entropy.clone().try_into().unwrap()) != uid
            || a.gains != cost
            || e["uid"] != uid
            || e["fraction"] != "player"
            || !e["isDeleted"].is_boolean()
            || number(e, "state") != 0
            || [
                "buildStep",
                "buildStartTime",
                "buildTotalTime",
                "productionType",
                "productionStartTime",
            ]
            .iter()
            .any(|k| number(e, k) != 0)
            || e["isInBattle"]
                != serde_json::Value::Bool(item.kind == "units" && self.unit_in_mission(uid))
            || ![SE, SW, NE, NW].contains(&(number(e, "direction") as u32))
            || ["x", "y"]
                .iter()
                .any(|k| e[k].as_i64().is_none_or(|n| !(0..128).contains(&n)))
        {
            return Err("Invalid shop entity purchase/identity".into());
        }
        if e["isDeleted"] != true && e["isInBattle"] != true {
            self.placement_type(
                scene,
                id,
                uid,
                number(e, "x") as i32 - 16,
                number(e, "y") as i32 - 16,
                number(e, "direction") as u32,
            )?;
        }
        self.catalog_entity(scene, e, None)?;
        Ok(())
    }
    pub(crate) fn validate_shop_accesses(&self, scene: &Scene) -> Result<(), String> {
        for a in &self.actions {
            if a.kind != "shop_access" {
                if a.shop.is_some() {
                    return Err("Access metadata on non-access action".into());
                }
                continue;
            }
            let item = scene
                .shop
                .as_ref()
                .and_then(|c| c.items.get(&a.recipe))
                .ok_or("Unknown shop access item")?;
            let r = a.shop.as_ref().ok_or("Missing shop access receipt")?;
            let increment = if [3001, 3002, 3010, 3011].contains(&item.id) {
                10
            } else {
                1
            };
            if a.uid != format!("shop:{}", item.id)
                || a.source.is_some()
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
                || a.at <= 0
                || a.at > self.last_action_time
                || !(0..=self.resources["level"]).contains(&r.level)
                || r.owned < 0
                || r.owned >= item.max_count(35)
                || r.previous < 0
                || r.previous > 65535
                || r.previous.checked_add(increment) != Some(r.granted)
                || item.unlock_price(r.owned)? <= 0
                || a.gains != BTreeMap::from([("realMoney".into(), -item.unlock_price(r.owned)?)])
                || self.shop_access(item) < r.granted
            {
                return Err("Invalid source shop access receipt".into());
            }
        }
        Ok(())
    }
}
