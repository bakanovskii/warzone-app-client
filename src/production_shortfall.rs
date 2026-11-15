//! NoResourcesDialog: quote missing recipe inputs, purchase, then start atomically.
use crate::{
    Scene,
    construction::Shortfall,
    production::{Command, State, adjust, number},
};
use std::collections::BTreeMap;
impl State {
    fn production_inputs(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        sequence: u64,
        owner: bool,
    ) -> Result<BTreeMap<String, i64>, String> {
        if let Some(r) = scene.research.as_ref().and_then(|c| c.items.get(&id)) {
            if owner
                && self
                    .entities
                    .get(uid)
                    .is_none_or(|e| number(e, "type") != 1020)
            {
                return Err("Исследованию нужен Исследовательский Центр".into());
            }
            let mut cost = r.cost.clone();
            for (id, n) in &r.items {
                cost.insert(format!("item:{id}"), *n);
            }
            Ok(cost)
        } else if let Some(r) = scene.workshop.as_ref().and_then(|c| c.recipes.get(&id)) {
            if owner {
                let e = self.entities.get(uid).ok_or("Нет здания")?;
                if !scene
                    .workshop
                    .as_ref()
                    .unwrap()
                    .buildings
                    .get(&(number(e, "type") as u32))
                    .is_some_and(|v| v.contains(&id))
                {
                    return Err("Рецепт не принадлежит зданию".into());
                }
            }
            let r = self.upgraded_recipe(scene, r, sequence);
            let mut cost = r.cost;
            for (id, n) in r.items {
                cost.insert(format!("item:{id}"), n);
            }
            Ok(cost)
        } else {
            let c = scene.economy.as_ref().ok_or("Нет каталога производства")?;
            if owner {
                let e = self.entities.get(uid).ok_or("Нет здания")?;
                if !c
                    .buildings
                    .get(&(number(e, "type") as u32))
                    .is_some_and(|b| b.recipes.contains(&id))
                {
                    return Err("Рецепт не принадлежит зданию".into());
                }
            }
            Ok(c.recipes.get(&id).ok_or("Нет рецепта")?.cost.clone())
        }
    }
    fn production_topup_price(scene: &Scene, key: &str, n: i64) -> Result<i64, String> {
        if let Some(id) = key.strip_prefix("item:") {
            crate::pricing::special_price(scene, id.parse().map_err(|_| "Invalid material ID")?)?
                .checked_mul(n)
                .ok_or("Resource price overflow".into())
        } else {
            scene
                .pricing
                .as_ref()
                .ok_or("Нет цен ресурсов")?
                .resource(key, n)
        }
    }
    pub fn production_shortfall(&self, scene: &Scene, cmd: &Command) -> Result<Shortfall, String> {
        let Command::Start { uid, recipe } = cmd else {
            return Err("Докупка доступна перед производством".into());
        };
        let mut resources = BTreeMap::new();
        let mut cash = 0i64;
        for (key, need) in self.production_inputs(scene, uid, *recipe, self.revision + 1, true)? {
            let have = if let Some(id) = key.strip_prefix("item:") {
                self.inventory.get(id).copied().unwrap_or(0)
            } else {
                self.resources.get(&key).copied().unwrap_or(0)
            };
            if need < 0 || have < 0 {
                return Err("Invalid recipe balance".into());
            }
            if key == "realMoney" {
                if have < need {
                    return Err("Недостаточно банкнот".into());
                }
                continue;
            }
            let n = (need - have).max(0);
            if n > 0 {
                cash = cash
                    .checked_add(Self::production_topup_price(scene, &key, n)?)
                    .ok_or("Resource price overflow")?;
                resources.insert(key, n);
            }
        }
        Ok(Shortfall { resources, cash })
    }
    /// Caller commits only this final state. Any failed access, payment, timer or
    /// start validation rolls back both resource purchase and production start.
    pub fn start_with_shortfall(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        let Command::Start { uid, recipe } = cmd else {
            return Err("Not a production start".into());
        };
        let q = self.production_shortfall(scene, cmd)?;
        let mut next = self.clone();
        if !q.resources.is_empty() {
            let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
            if now < self.last_action_time {
                return Err("Часы переведены назад".into());
            }
            next.charge_cash(q.cash)?;
            let mut gains = q.resources.clone();
            gains.insert("realMoney".into(), -q.cash);
            for (key, n) in q.resources {
                if let Some(id) = key.strip_prefix("item:") {
                    adjust(&mut next.inventory, id, n)?;
                } else {
                    adjust(&mut next.resources, &key, n)?;
                }
            }
            next.building_receipt(
                now,
                uid,
                *recipe,
                "production_resource_topup",
                gains,
                vec![],
            )?;
        }
        if scene
            .research
            .as_ref()
            .is_some_and(|c| c.items.contains_key(recipe))
        {
            next.research_apply(scene, cmd, wall)
        } else if scene
            .workshop
            .as_ref()
            .is_some_and(|c| c.recipes.contains_key(recipe))
        {
            next.workshop_apply(scene, cmd, wall, entropy)
        } else {
            next.apply(scene, cmd, wall, entropy)
        }
    }
    pub(crate) fn validate_production_topups(&self, scene: &Scene) -> Result<(), String> {
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "production_resource_topup")
        {
            let inputs = self.production_inputs(scene, &a.uid, a.recipe, a.sequence + 1, false)?;
            let mut cash = 0i64;
            for (k, n) in &a.gains {
                if k == "realMoney" {
                    continue;
                }
                if *n <= 0 || *n > inputs.get(k).copied().unwrap_or(0) {
                    return Err("Invalid production topup amount".into());
                }
                cash = cash
                    .checked_add(Self::production_topup_price(scene, k, *n)?)
                    .ok_or("Price overflow")?;
            }
            if a.gains.len() < 2
                || a.gains.get("realMoney") != Some(&-cash)
                || !a.entropy.is_empty()
                || !self.actions.iter().any(|b| {
                    b.sequence == a.sequence + 1
                        && b.uid == a.uid
                        && b.recipe == a.recipe
                        && b.at == a.at
                        && matches!(
                            b.kind.as_str(),
                            "start" | "workshop_start" | "research_start"
                        )
                })
            {
                return Err("Unpaired production topup".into());
            }
        }
        Ok(())
    }
}
