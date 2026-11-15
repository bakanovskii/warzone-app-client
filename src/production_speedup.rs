//! The production branch of SceneMain.onSpeedUp, shared by research/workshops.
//! A purchase forces the source timer; collection/placement remains separate.
use crate::{
    Scene,
    production::{ProductionSource, State, number},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Construction,
    Resource,
    Research,
    Workshop,
    Upgrade,
}
impl Kind {
    fn receipt(self) -> &'static str {
        match self {
            Self::Construction => "building_speedup",
            Self::Resource => "resource_speedup",
            Self::Research => "research_speedup",
            Self::Workshop => "workshop_speedup",
            Self::Upgrade => "upgrade_speedup",
        }
    }
}
pub struct Quote {
    pub kind: Kind,
    pub recipe: u32,
    pub started: i64,
    pub remaining: i64,
    pub cash: i64,
}
fn recipe(
    state: &State,
    scene: &Scene,
    building: u32,
    id: u32,
    sequence: u64,
) -> Option<(Kind, i64)> {
    if building == 1019 {
        return scene
            .upgrades
            .as_ref()?
            .items
            .get(&id)
            .map(|r| (Kind::Upgrade, r.seconds));
    }
    if building == 1020 {
        return scene
            .research
            .as_ref()?
            .items
            .get(&id)
            .map(|r| (Kind::Research, r.seconds));
    }
    let c = scene.workshop.as_ref()?;
    if !c.buildings.get(&building)?.contains(&id) {
        return None;
    }
    c.recipes.get(&id).map(|r| {
        (
            Kind::Workshop,
            state.upgraded_recipe(scene, r, sequence).seconds,
        )
    })
}
impl State {
    pub fn production_speedup_quote(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<Quote, String> {
        if self.construction_pending(scene, uid) {
            return self.construction_speedup_quote(scene, uid, wall);
        }
        if let Ok(q) = self.resource_speedup_quote(scene, uid, wall) {
            return Ok(Quote {
                kind: Kind::Resource,
                recipe: q.recipe,
                started: q.started,
                remaining: q.remaining,
                cash: q.cash,
            });
        }
        let e = self.entities.get(uid).ok_or("Здание не найдено")?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        if number(e, "state") != 3 || e["isDeleted"] == true || e["isInBattle"] == true {
            return Err("Здание сейчас недоступно".into());
        }
        let id = number(e, "productionType") as u32;
        let started = number(e, "productionStartTime");
        let sequence = self.workshop_origin_sequence(uid, id, started, u64::MAX);
        let (kind, seconds) = recipe(self, scene, number(e, "type") as u32, id, sequence)
            .ok_or("Нет активного исследования или производства")?;
        let remaining = started
            .checked_add(seconds)
            .and_then(|n| n.checked_sub(now))
            .ok_or("Timer overflow")?;
        if !(1..864000).contains(&seconds) || started <= 0 || started > now || remaining < 0 {
            return Err("Производство уже готово или таймер изменился".into());
        }
        let cash = scene
            .pricing
            .as_ref()
            .ok_or("Missing original prices")?
            .time(remaining)?;
        Ok(Quote {
            kind,
            recipe: id,
            started,
            remaining,
            cash,
        })
    }
    pub fn speedup_production(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let q = self.production_speedup_quote(scene, uid, wall)?;
        if q.kind == Kind::Construction {
            return self.speedup_construction(scene, uid, wall);
        }
        if q.kind == Kind::Resource {
            return self.speedup_resource(scene, uid, wall);
        }
        if scene.progression.as_ref().is_some_and(|c| {
            c.quests
                .iter()
                .any(|q| q.exercises.iter().any(|e| e.kind == "speed_up"))
        }) {
            return Err("Нужна исходная идентичность задания ускорения".into());
        }
        let now = self.clock(scene.economy.as_ref().unwrap(), wall)?;
        let forced = now
            .checked_sub(864000)
            .filter(|n| *n > 0)
            .ok_or("Время вне диапазона ускорения")?;
        let mut next = self.clone();
        next.charge_cash(q.cash)?;
        next.entities.get_mut(uid).unwrap()["productionStartTime"] = forced.into();
        next.building_receipt(
            now,
            uid,
            q.recipe,
            q.kind.receipt(),
            BTreeMap::from([("realMoney".into(), -q.cash)]),
            vec![],
        )?;
        next.actions.last_mut().unwrap().source = Some(ProductionSource {
            uid: uid.into(),
            started: q.started,
        });
        next.validate(scene)?;
        Ok((next, "Готово к получению".into()))
    }
    /// Keep the original paid start for origin validation after forceProduction
    /// replaces the live timestamp. Every matching force receipt is validated.
    pub(crate) fn production_origin(&self, uid: &str, id: u32, started: i64, before: u64) -> i64 {
        self.actions
            .iter()
            .find(|a| {
                a.sequence < before
                    && a.uid == uid
                    && a.recipe == id
                    && matches!(
                        a.kind.as_str(),
                        "workshop_speedup" | "research_speedup" | "upgrade_speedup"
                    )
                    && a.at.checked_sub(864000) == Some(started)
            })
            .and_then(|a| a.source.as_ref())
            .map_or(started, |s| s.started)
    }
    pub(crate) fn production_forced_start(
        &self,
        uid: &str,
        id: u32,
        started: i64,
        before: u64,
    ) -> i64 {
        self.actions
            .iter()
            .find(|a| {
                a.sequence < before
                    && a.uid == uid
                    && a.recipe == id
                    && matches!(
                        a.kind.as_str(),
                        "workshop_speedup"
                            | "research_speedup"
                            | "upgrade_speedup"
                            | "resource_speedup"
                    )
                    && a.source.as_ref().is_some_and(|s| s.started == started)
            })
            .map_or(started, |a| a.at - 864000)
    }
    pub(crate) fn validate_production_speedups(&self, scene: &Scene) -> Result<(), String> {
        let mut used = BTreeSet::new();
        for a in self.actions.iter().filter(|a| {
            matches!(
                a.kind.as_str(),
                "research_speedup" | "workshop_speedup" | "upgrade_speedup"
            )
        }) {
            let src = a
                .source
                .as_ref()
                .ok_or("Missing forced production origin")?;
            let e = self
                .entities
                .get(&a.uid)
                .ok_or("Missing forced production building")?;
            let sequence = self.workshop_origin_sequence(&a.uid, a.recipe, src.started, a.sequence);
            let (kind, seconds) = recipe(self, scene, number(e, "type") as u32, a.recipe, sequence)
                .ok_or("Invalid forced production recipe")?;
            let remaining = src
                .started
                .checked_add(seconds)
                .and_then(|n| n.checked_sub(a.at))
                .ok_or("Invalid forced production timer")?;
            let price = scene
                .pricing
                .as_ref()
                .ok_or("Missing force price")?
                .time(remaining)?;
            let native = self.actions.iter().any(|b| {
                b.uid == a.uid
                    && b.recipe == a.recipe
                    && b.at == src.started
                    && b.sequence < a.sequence
                    && b.kind
                        == match kind {
                            Kind::Research => "research_start",
                            Kind::Upgrade => "upgrade_start",
                            _ => "workshop_start",
                        }
            });
            let imported = scene.entities.iter().any(|e| {
                e.uid == a.uid
                    && number(&e.archived, "productionType") == a.recipe as i64
                    && number(&e.archived, "productionStartTime") == src.started
            });
            if a.kind != kind.receipt()
                || src.uid != a.uid
                || src.started <= 0
                || src.started > a.at
                || a.at <= 864000
                || a.at > self.last_action_time
                || remaining < 0
                || !(1..864000).contains(&seconds)
                || !used.insert((a.uid.clone(), src.started))
                || (!native && !imported)
                || a.gains != BTreeMap::from([("realMoney".into(), -price)])
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid forced production receipt".into());
            }
        }
        Ok(())
    }
}
