//! SceneMain.onSpeedUp + Building.forceProduction for resource contracts.
//! Paying advances the timer; rewards remain uncollected in the building.
use crate::{
    Scene,
    production::{ProductionSource, State, number},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug)]
pub struct Quote {
    pub recipe: u32,
    pub started: i64,
    pub remaining: i64,
    pub cash: i64,
}
impl State {
    pub fn resource_speedup_quote(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<Quote, String> {
        let e = self.entities.get(uid).ok_or("Здание не найдено")?;
        let ec = scene.economy.as_ref().ok_or("Missing resource contracts")?;
        let now = self.clock(ec, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        if number(e, "state") != 3 || e["isDeleted"] == true || e["isInBattle"] == true {
            return Err("Здание сейчас недоступно".into());
        }
        let id = number(e, "productionType") as u32;
        let r = ec
            .recipes
            .get(&id)
            .filter(|r| r.seconds > 0 && r.seconds < 864000)
            .ok_or("Нет активного ресурсного контракта")?;
        if !ec
            .buildings
            .get(&(number(e, "type") as u32))
            .is_some_and(|b| b.recipes.contains(&id))
        {
            return Err("Контракт не принадлежит этому зданию".into());
        }
        let started = number(e, "productionStartTime");
        let remaining = started
            .checked_add(r.seconds)
            .and_then(|v| v.checked_sub(now))
            .ok_or("Timer overflow")?;
        if started <= 0 || started > now || remaining < 0 {
            return Err("Производство уже готово или таймер изменился".into());
        }
        let cash = scene
            .pricing
            .as_ref()
            .ok_or("Нет исходного расчёта ускорения")?
            .time(remaining)?;
        Ok(Quote {
            recipe: id,
            started,
            remaining,
            cash,
        })
    }
    pub fn speedup_resource(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let q = self.resource_speedup_quote(scene, uid, wall)?;
        // Source event7 uses Building.nRecipeFinishedID, an ephemeral onTouch field,
        // not the current recipe. Current source quests have NO speed_up exercises.
        // Do not invent an identity or silently change future packs' objectives.
        if scene.progression.as_ref().is_some_and(|c| {
            c.quests
                .iter()
                .any(|q| q.exercises.iter().any(|e| e.kind == "speed_up"))
        }) {
            return Err("Нужна исходная идентичность задания ускорения".into());
        }
        let now = self.clock(scene.economy.as_ref().unwrap(), wall)?;
        let forced = now
            .checked_sub(0xd2f00)
            .filter(|n| *n > 0)
            .ok_or("Время вне диапазона ускорения")?;
        let mut next = self.clone();
        next.charge_cash(q.cash)?;
        next.entities.get_mut(uid).unwrap()["productionStartTime"] = forced.into();
        next.building_receipt(
            now,
            uid,
            q.recipe,
            "resource_speedup",
            BTreeMap::from([("realMoney".into(), -q.cash)]),
            vec![],
        )?;
        next.actions.last_mut().unwrap().source = Some(ProductionSource {
            uid: uid.into(),
            started: q.started,
        });
        next.validate(scene)?;
        Ok((next, "Производство готово к сбору".into()))
    }
    pub(crate) fn validate_resource_speedups(&self, scene: &Scene) -> Result<(), String> {
        let mut sources = BTreeSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "resource_speedup") {
            let src = a.source.as_ref().ok_or("Missing speedup timer source")?;
            let r = scene
                .economy
                .as_ref()
                .and_then(|c| c.recipes.get(&a.recipe))
                .ok_or("Unknown speedup recipe")?;
            let e = self
                .entities
                .get(&a.uid)
                .ok_or("Missing speedup building")?;
            let remaining = src
                .started
                .checked_add(r.seconds)
                .and_then(|n| n.checked_sub(a.at))
                .ok_or("Invalid speedup timer")?;
            let price = scene
                .pricing
                .as_ref()
                .ok_or("Missing speedup prices")?
                .time(remaining)?;
            if src.uid != a.uid
                || src.started <= 0
                || a.at < src.started
                || a.at <= 0xd2f00
                || remaining < 0
                || !sources.insert((a.uid.clone(), src.started))
                || !scene
                    .economy
                    .as_ref()
                    .unwrap()
                    .buildings
                    .get(&(number(e, "type") as u32))
                    .is_some_and(|b| b.recipes.contains(&a.recipe))
                || a.gains != BTreeMap::from([("realMoney".into(), -price)])
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid resource speedup receipt".into());
            }
        }
        Ok(())
    }
}
