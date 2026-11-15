//! SceneMain.onSpeedUp's construction branch and Building.forceBuilding.
use crate::{
    Scene,
    production::{ProductionSource, State, number},
    production_speedup::{Kind, Quote},
};
use std::collections::{BTreeMap, BTreeSet};
impl State {
    pub fn construction_speedup_quote(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<Quote, String> {
        if !self.construction_pending(scene, uid) {
            return Err("Строительство сейчас не активно".into());
        }
        let e = &self.entities[uid];
        if e["isInBattle"] == true {
            return Err("Здание сейчас недоступно".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let started = number(e, "buildStartTime");
        let seconds = number(e, "buildTotalTime");
        let remaining = started
            .checked_add(seconds)
            .and_then(|n| n.checked_sub(now))
            .ok_or("Construction timer overflow")?;
        if !(1..864000).contains(&seconds) || started <= 0 || started > now || remaining <= 0 {
            return Err("Строительство уже готово или таймер изменился".into());
        }
        Ok(Quote {
            kind: Kind::Construction,
            recipe: number(e, "type") as u32,
            started,
            remaining,
            cash: scene
                .pricing
                .as_ref()
                .ok_or("Missing source speed prices")?
                .time(remaining)?,
        })
    }
    pub fn speedup_construction(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let q = self.construction_speedup_quote(scene, uid, wall)?;
        let now = self.clock(scene.economy.as_ref().unwrap(), wall)?;
        let forced = now
            .checked_sub(864000)
            .filter(|n| *n > 0)
            .ok_or("Время вне диапазона ускорения")?;
        let mut next = self.clone();
        next.charge_cash(q.cash)?;
        next.entities.get_mut(uid).unwrap()["buildStartTime"] = forced.into();
        next.building_receipt(
            now,
            uid,
            q.recipe,
            "building_speedup",
            BTreeMap::from([("realMoney".into(), -q.cash)]),
            vec![],
        )?;
        next.actions.last_mut().unwrap().source = Some(ProductionSource {
            uid: uid.into(),
            started: q.started,
        });
        next.validate(scene)?;
        Ok((next, "Здание готово к завершению".into()))
    }
    pub(crate) fn forced_creation_start(&self, uid: &str, started: i64) -> i64 {
        self.actions
            .iter()
            .find(|a| {
                a.uid == uid
                    && a.kind == "building_speedup"
                    && a.source.as_ref().is_some_and(|s| s.started == started)
            })
            .map_or(started, |a| a.at - 864000)
    }
    pub(crate) fn validate_construction_speedups(&self, scene: &Scene) -> Result<(), String> {
        let mut used = BTreeSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "building_speedup") {
            let src = a
                .source
                .as_ref()
                .ok_or("Missing forced construction origin")?;
            let e = self
                .entities
                .get(&a.uid)
                .ok_or("Missing forced construction building")?;
            let seconds = number(e, "buildTotalTime");
            let b = scene
                .construction
                .as_ref()
                .and_then(|c| c.buildings.get(&a.recipe))
                .ok_or("Unknown forced building")?;
            let remaining = src
                .started
                .checked_add(seconds)
                .and_then(|n| n.checked_sub(a.at))
                .ok_or("Invalid forced construction timer")?;
            let price = scene
                .pricing
                .as_ref()
                .ok_or("Missing construction force prices")?
                .time(remaining)?;
            let native = self.actions.iter().any(|b| {
                b.uid == a.uid
                    && b.recipe == a.recipe
                    && b.kind == "building_buy"
                    && b.at == src.started
                    && b.sequence < a.sequence
            });
            let imported = scene.entities.iter().any(|s| {
                s.uid == a.uid
                    && [1, 2].contains(&number(&s.archived, "state"))
                    && number(&s.archived, "buildStartTime") == src.started
            });
            if src.uid != a.uid
                || number(e, "type") != a.recipe as i64
                || src.started <= 0
                || src.started > a.at
                || a.at <= 864000
                || a.at > self.last_action_time
                || !(1..864000).contains(&seconds)
                || !b.times.values().any(|n| *n == seconds)
                || remaining <= 0
                || !used.insert(a.uid.clone())
                || (!native && !imported)
                || number(e, "buildStartTime") != a.at - 864000
                || a.gains != BTreeMap::from([("realMoney".into(), -price)])
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
                || self.actions.iter().any(|b| {
                    b.uid == a.uid && b.kind == "building_finish" && b.sequence <= a.sequence
                })
            {
                return Err("Invalid forced construction receipt".into());
            }
        }
        Ok(())
    }
}
