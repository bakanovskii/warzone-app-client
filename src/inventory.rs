//! SpecialItems/UserProfile inventory and ShopDialog's immediate resource packs.
use crate::{
    Scene,
    production::{Action, State, adjust},
    systems::SpecialItem,
};
use std::collections::BTreeMap;
impl State {
    /// addSpecialItem:quantity: initializes an ABSENT key to literal "1", even
    /// for quantity >1. Retain that original behavior, verified at0x33b6c.
    pub fn add_special_item(&mut self, id: u32, delta: i64) -> Result<i64, String> {
        let key = id.to_string();
        let old = self.inventory.get(&key).copied();
        let new = if let Some(n) = old {
            n.checked_add(delta).ok_or("Inventory overflow")?.max(0)
        } else {
            1
        };
        if new >= i32::MAX as i64 {
            return Err("Inventory quantity outside supported range".into());
        }
        self.inventory.insert(key, new);
        Ok(new - old.unwrap_or(0))
    }
    pub fn shop_items<'a>(&self, scene: &'a Scene) -> Vec<&'a SpecialItem> {
        let mut items: Vec<_> = scene
            .progression
            .as_ref()
            .map(|c| {
                c.special_items
                    .values()
                    .filter(|i| !i.outputs.is_empty() && !i.cost.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        items.sort_by_key(|i| (i.order, i.id));
        items
    }
    pub fn buy_special(&self, scene: &Scene, id: u32, wall: i64) -> Result<(Self, String), String> {
        let item = self
            .shop_items(scene)
            .into_iter()
            .find(|i| i.id == id)
            .ok_or("Этот предмет не продаётся в ресурсном магазине")?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        if id == 4000 && self.energy(scene, now) == self.max_energy(scene) {
            return Err("Энергия уже полностью восстановлена".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        for (key, price) in &item.cost {
            if *price < 0 || !["realMoney", "playMoney", "fuel", "iron"].contains(&key.as_str()) {
                return Err("Неподдерживаемая цена предмета".into());
            }
            if next.resources.get(key).copied().unwrap_or(0) < *price {
                return Err("Недостаточно ресурсов для покупки".into());
            }
            adjust(&mut next.resources, key, -price)?;
            if key == "realMoney" {
                let sys = next.systems.as_mut().ok_or("Missing free cash state")?;
                sys.set(
                    "realMoneyFree".into(),
                    (sys.integer("realMoneyFree") - price).max(0),
                );
            }
        }
        // ShopDialog's resource packs do NOT enforce production storage caps.
        // They apply immediately and are never stored as consumable inventory.
        for award in &item.outputs {
            next.award(award, scene, now, &mut gains)?;
        }
        next.revision = next.revision.checked_add(1).ok_or("Revision overflow")?;
        next.last_action_time = now;
        next.systems
            .as_mut()
            .ok_or("Missing inventory state")?
            .applied_action_revision = next.revision;
        next.actions.push(Action {
            at_ms: None,
            shop: None,
            source: None,
            sequence: next.revision,
            at: now,
            uid: format!("shop:{id}"),
            recipe: id,
            kind: "shop_buy".into(),
            gains,
            entropy: vec![],
            quest_events_pending: vec![],
        });
        next.sync_preferences();
        next.validate(scene)?;
        Ok((next, format!("Куплено: {}", item.name)))
    }
}

impl State {
    pub fn needs_recipe_unlock(&self, scene: &Scene, recipe: u32) -> bool {
        scene
            .economy
            .as_ref()
            .and_then(|c| c.recipes.get(&recipe))
            .is_some_and(|r| self.resources["level"] < r.min_level)
            && !self
                .systems
                .as_ref()
                .is_some_and(|s| s.flag(&format!("RecipeUnlock-{recipe}")))
    }
    pub fn unlock_recipe(
        &self,
        scene: &Scene,
        uid: &str,
        recipe: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let config = scene.economy.as_ref().ok_or("Missing recipes")?;
        let e = self.entities.get(uid).ok_or("Здание не найдено")?;
        let r = config.recipes.get(&recipe).ok_or("Рецепт не найден")?;
        if crate::production::number(e, "state") != 3
            || e["isDeleted"] == true
            || crate::production::number(e, "productionType") != 0
            || !config
                .buildings
                .get(&(crate::production::number(e, "type") as u32))
                .is_some_and(|b| b.recipes.contains(&recipe))
        {
            return Err("Это здание сейчас не может открыть выбранный рецепт".into());
        }
        if !self.needs_recipe_unlock(scene, recipe) {
            return Err("Рецепт уже доступен".into());
        }
        let price = r
            .unlock_price
            .filter(|n| *n >= 0)
            .ok_or("Цена разблокировки недоступна в этом пакете")?;
        if self.resources["realMoney"] < price {
            return Err("Недостаточно кэша".into());
        }
        let now = self.clock(config, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        adjust(&mut next.resources, "realMoney", -price)?;
        let sys = next.systems.as_mut().ok_or("Missing recipe state")?;
        sys.set(
            "realMoneyFree".into(),
            (sys.integer("realMoneyFree") - price).max(0),
        );
        sys.preferences
            .insert(format!("RecipeUnlock-{recipe}"), true.into());
        next.revision = next.revision.checked_add(1).ok_or("Revision overflow")?;
        sys.applied_action_revision = next.revision;
        next.last_action_time = now;
        next.actions.push(Action {
            at_ms: None,
            shop: None,
            source: None,
            sequence: next.revision,
            at: now,
            uid: uid.into(),
            recipe,
            kind: "recipe_unlock".into(),
            gains: BTreeMap::new(),
            entropy: vec![],
            quest_events_pending: vec![],
        });
        next.sync_preferences();
        next.validate(scene)?;
        Ok((next, "Рецепт открыт и сохранён".into()))
    }
}
