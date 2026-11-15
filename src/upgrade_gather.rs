//! SceneMain.onUpgradesTouch and delayed block0x8bf31: six-second gather.
//! Flags/modifiers apply at6s, quest event at the later bonus flight.
//! Legacy upgrade_finish receipts remain valid; no new serialized State fields.
use crate::callback_clock::{CallbackTime, flight_delay, time};
use crate::{
    Scene,
    production::{Command, State, number},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
const KEY: &str = "__native_upgrade_gathers";
pub const SECONDS: i64 = 6;
// Source drop .05+.3+.2+.1+.1, then lifetimeMode3 waits3s. Saved
// transaction clocks have1second precision: first eligible tick is+4.
pub const FLIGHT_TICK: i64 = 4;
fn is_false(v: &bool) -> bool {
    !*v
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Pending {
    pub recipe: u32,
    pub started: i64,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flight_event: bool,
}
impl State {
    pub fn pending_upgrade_gathers(&self) -> Result<BTreeMap<String, Pending>, String> {
        match self.systems.as_ref().and_then(|s| s.preferences.get(KEY)) {
            None => Ok(BTreeMap::new()),
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|_| "Invalid upgrade gathering queue".into()),
        }
    }
    pub fn upgrade_gathering(&self, uid: &str) -> bool {
        self.systems
            .as_ref()
            .and_then(|s| s.preferences.get(KEY))
            .is_some_and(|v| v.get(uid).is_some())
    }
    fn save_upgrade_gathers(&mut self, queue: &BTreeMap<String, Pending>) -> Result<(), String> {
        let p = &mut self
            .systems
            .as_mut()
            .ok_or("Missing upgrade preferences")?
            .preferences;
        if queue.is_empty() {
            p.remove(KEY);
        } else {
            p.insert(
                KEY.into(),
                serde_json::to_value(queue).map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }
    /// Ordinary application path. Start keeps its established transaction;
    /// a ready collect starts the source gather instead of granting immediately.
    pub fn upgrade_ui_apply(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.upgrade_ui_apply_at(scene, cmd, clock)
    }
    pub fn upgrade_ui_apply_ms(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall_ms: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.upgrade_ui_apply_at(scene, cmd, clock)
    }
    fn upgrade_ui_apply_at(
        &self,
        scene: &Scene,
        cmd: &Command,
        clock: CallbackTime,
    ) -> Result<(Self, String), String> {
        let wall = clock.wall;
        clock.check_order(self)?;
        let Command::Collect {
            uid,
            recipe,
            started,
        } = cmd
        else {
            return self.upgrade_apply(scene, cmd, wall);
        };
        // Reuse established original eligibility/cost/timer validation on a
        // discarded candidate. The actual flag/event is only committed later.
        self.upgrade_apply(scene, cmd, wall)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now
            .checked_add(SECONDS)
            .is_none_or(|t| t >= i32::MAX as i64)
        {
            return Err("Недопустимое время получения улучшения".into());
        }
        let mut queue = self.pending_upgrade_gathers()?;
        if queue
            .insert(
                uid.clone(),
                Pending {
                    recipe: *recipe,
                    started: *started,
                    at: now,
                    at_ms: clock.stamp(clock.precise),
                    flight_event: true,
                },
            )
            .is_some()
        {
            return Err("Улучшение уже получается".into());
        }
        let mut next = self.clone();
        next.selected = None;
        next.save_upgrade_gathers(&queue)?;
        next.building_receipt(
            now,
            uid,
            *recipe,
            "gather_upgrade_flight",
            BTreeMap::new(),
            vec![],
        )?;
        next.actions.last_mut().unwrap().at_ms = clock.stamp(clock.precise);
        next.validate(scene)?;
        Ok((next, "Получение улучшения".into()))
    }
    pub fn upgrade_gathers_due(&self, scene: &Scene, wall: i64) -> Result<bool, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.upgrade_gathers_due_at(scene, clock)
    }
    pub fn upgrade_gathers_due_ms(&self, scene: &Scene, wall_ms: i64) -> Result<bool, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.upgrade_gathers_due_at(scene, clock)
    }
    fn upgrade_gathers_due_at(&self, _scene: &Scene, clock: CallbackTime) -> Result<bool, String> {
        Ok(self
            .pending_upgrade_gathers()?
            .values()
            .any(|p| clock.now_ms >= p.at_ms.unwrap_or(p.at * 1000) + SECONDS * 1000))
    }
    pub fn settle_upgrade_gathers(&self, scene: &Scene, wall: i64) -> Result<Option<Self>, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.settle_upgrade_gathers_at(scene, clock)
    }
    pub fn settle_upgrade_gathers_ms(
        &self,
        scene: &Scene,
        wall_ms: i64,
    ) -> Result<Option<Self>, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.settle_upgrade_gathers_at(scene, clock)
    }
    fn settle_upgrade_gathers_at(
        &self,
        scene: &Scene,
        clock: CallbackTime,
    ) -> Result<Option<Self>, String> {
        let wall = clock.wall;
        if !self.upgrade_gathers_due_at(scene, clock)? {
            return Ok(None);
        }
        self.validate(scene)?;
        clock.check_order(self)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let mut queue = self.pending_upgrade_gathers()?;
        let due: Vec<_> = queue
            .iter()
            .filter(|(_, p)| clock.now_ms >= p.at_ms.unwrap_or(p.at * 1000) + SECONDS * 1000)
            .map(|(uid, p)| (uid.clone(), p.clone()))
            .collect();
        for (uid, p) in due {
            queue.remove(&uid);
            next.save_upgrade_gathers(&queue)?;
            // The source callback runs after onUpgradesTouch even if another
            // scene is opened. Keep active mission/session state untouched.
            next = next
                .upgrade_apply_background(
                    scene,
                    &Command::Collect {
                        uid,
                        recipe: p.recipe,
                        started: p.started,
                    },
                    wall,
                    p.flight_event,
                    clock.stamp(p.at_ms.is_some()),
                )?
                .0;
        }
        next.validate(scene)?;
        Ok(Some(next))
    }
    pub(crate) fn validate_upgrade_gathers(&self, scene: &Scene) -> Result<(), String> {
        let pending = self.pending_upgrade_gathers()?;
        let mut expected = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| matches!(a.kind.as_str(), "gather_upgrade" | "gather_upgrade_flight"))
        {
            let u = scene
                .upgrades
                .as_ref()
                .and_then(|c| c.items.get(&a.recipe))
                .ok_or("Unknown gathered upgrade")?;
            let e = self.entities.get(&a.uid).ok_or("Missing gather academy")?;
            if number(e, "type") != 1019
                || !seen.insert(a.recipe)
                || a.at <= 0
                || a.at > self.last_action_time
                || !a.gains.is_empty()
                || !a.entropy.is_empty()
                || a.source.is_some()
                || a.shop.is_some()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid/duplicate upgrade gather receipt".into());
            }
            let start = self
                .actions
                .iter()
                .rev()
                .find(|s| {
                    s.kind == "upgrade_start"
                        && s.uid == a.uid
                        && s.recipe == a.recipe
                        && s.sequence < a.sequence
                })
                .map(|s| s.at)
                .or_else(|| {
                    scene
                        .entities
                        .iter()
                        .find(|s| {
                            s.uid == a.uid
                                && number(&s.archived, "productionType") == a.recipe as i64
                        })
                        .map(|s| number(&s.archived, "productionStartTime"))
                })
                .ok_or("Upgrade gathering without original production")?;
            let started = self.production_forced_start(&a.uid, a.recipe, start, a.sequence);
            if a.at <= started + u.seconds {
                return Err("Upgrade gathered before ready".into());
            }
            let finishes: Vec<_> = self
                .actions
                .iter()
                .filter(|f| f.kind == "upgrade_finish" && f.recipe == a.recipe)
                .collect();
            if let Some(f) = finishes.first() {
                if finishes.len() != 1
                    || f.uid != a.uid
                    || f.sequence <= a.sequence
                    || time(f) < time(a) + SECONDS * 1000
                    || f.at_ms.is_some() != a.at_ms.is_some()
                {
                    return Err("Upgrade completed before gather delay".into());
                }
            } else {
                if e["isDeleted"] == true
                    || e["isInBattle"] == true
                    || self.upgrade_ready(a.recipe)
                    || number(e, "productionType") != a.recipe as i64
                    || number(e, "productionStartTime") != started
                    || expected
                        .insert(
                            a.uid.clone(),
                            Pending {
                                recipe: a.recipe,
                                started,
                                at: a.at,
                                at_ms: a.at_ms,
                                flight_event: a.kind == "gather_upgrade_flight",
                            },
                        )
                        .is_some()
                {
                    return Err("Invalid gathering academy state".into());
                }
            }
        }
        if pending != expected {
            return Err("Upgrade gather queue differs from history".into());
        }
        let mut flown = BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| matches!(a.kind.as_str(), "flight_upgrade" | "flight_upgrade_touch"))
        {
            let finished = self
                .actions
                .iter()
                .find(|f| f.kind == "upgrade_finish" && f.uid == a.uid && f.recipe == a.recipe)
                .ok_or("Upgrade flight without finished upgrade")?;
            let gather = self
                .actions
                .iter()
                .find(|g| {
                    g.kind == "gather_upgrade_flight" && g.uid == a.uid && g.recipe == a.recipe
                })
                .ok_or("Upgrade flight without source gather")?;
            if !flown.insert(a.recipe)
                || a.sequence <= finished.sequence
                || finished.sequence <= gather.sequence
                || time(a)
                    < time(finished)
                        + if a.kind == "flight_upgrade_touch" {
                            0
                        } else {
                            flight_delay(finished)
                        }
                || a.at_ms.is_some() != finished.at_ms.is_some()
                || a.at > self.last_action_time
                || !a.gains.is_empty()
                || !a.entropy.is_empty()
                || a.source.is_some()
                || a.shop.is_some()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid/repeated upgrade flight event".into());
            }
        }
        Ok(())
    }
    pub fn pending_upgrade_flights(&self) -> Vec<&crate::production::Action> {
        self.actions
            .iter()
            .filter(|f| {
                f.kind == "upgrade_finish"
                    && self.actions.iter().any(|g| {
                        g.kind == "gather_upgrade_flight"
                            && g.uid == f.uid
                            && g.recipe == f.recipe
                            && g.sequence < f.sequence
                    })
                    && !self.actions.iter().any(|a| {
                        matches!(a.kind.as_str(), "flight_upgrade" | "flight_upgrade_touch")
                            && a.recipe == f.recipe
                    })
            })
            .collect()
    }
    pub fn upgrade_flights_due(&self, scene: &Scene, wall: i64) -> Result<bool, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.upgrade_flights_due_at(scene, clock)
    }
    pub fn upgrade_flights_due_ms(&self, scene: &Scene, wall_ms: i64) -> Result<bool, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.upgrade_flights_due_at(scene, clock)
    }
    fn upgrade_flights_due_at(&self, _scene: &Scene, clock: CallbackTime) -> Result<bool, String> {
        Ok(self
            .pending_upgrade_flights()
            .iter()
            .any(|a| clock.now_ms >= time(a) + flight_delay(a)))
    }
    pub fn settle_upgrade_flights(&self, scene: &Scene, wall: i64) -> Result<Option<Self>, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.settle_upgrade_flights_at(scene, clock)
    }
    pub fn settle_upgrade_flights_ms(
        &self,
        scene: &Scene,
        wall_ms: i64,
    ) -> Result<Option<Self>, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.settle_upgrade_flights_at(scene, clock)
    }
    fn settle_upgrade_flights_at(
        &self,
        scene: &Scene,
        clock: CallbackTime,
    ) -> Result<Option<Self>, String> {
        let wall = clock.wall;
        if !self.upgrade_flights_due_at(scene, clock)? {
            return Ok(None);
        }
        self.validate(scene)?;
        clock.check_order(self)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let due: Vec<_> = self
            .pending_upgrade_flights()
            .into_iter()
            .filter(|a| clock.now_ms >= time(a) + flight_delay(a))
            .map(|a| (a.uid.clone(), a.recipe))
            .collect();
        let mut next = self.clone();
        for (_, id) in due {
            next = next.finish_upgrade_flight(scene, id, clock, false)?.0;
        }
        next.validate(scene)?;
        Ok(Some(next))
    }
    pub fn pickup_upgrade_bonus(
        &self,
        scene: &Scene,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.pickup_upgrade_bonus_at(scene, id, clock)
    }
    pub fn pickup_upgrade_bonus_ms(
        &self,
        scene: &Scene,
        id: u32,
        wall_ms: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.pickup_upgrade_bonus_at(scene, id, clock)
    }
    pub(crate) fn pickup_upgrade_bonus_at(
        &self,
        scene: &Scene,
        id: u32,
        clock: CallbackTime,
    ) -> Result<(Self, String), String> {
        self.finish_upgrade_flight(scene, id, clock, true)
    }
    fn finish_upgrade_flight(
        &self,
        scene: &Scene,
        id: u32,
        clock: CallbackTime,
        touch: bool,
    ) -> Result<(Self, String), String> {
        let wall = clock.wall;
        self.validate(scene)?;
        clock.check_order(self)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        let source = self
            .pending_upgrade_flights()
            .into_iter()
            .find(|a| a.recipe == id)
            .ok_or("Бонус уже подобран")?;
        if now < self.last_action_time
            || clock.now_ms < time(source) + if touch { 0 } else { flight_delay(source) }
        {
            return Err("Бонус ещё не готов к подбору".into());
        }
        let mut next = self.clone();
        if let Some(c) = &scene.progression {
            next.systems.as_mut().ok_or("Missing quest state")?.event(
                c,
                next.resources["level"],
                23,
                id,
            )?;
        }
        next.building_receipt(
            now,
            &source.uid,
            id,
            if touch {
                "flight_upgrade_touch"
            } else {
                "flight_upgrade"
            },
            BTreeMap::new(),
            vec![],
        )?;
        next.actions.last_mut().unwrap().at_ms = clock.stamp(source.at_ms.is_some());
        next.validate(scene)?;
        Ok((next, "Бонус улучшения получен".into()))
    }
}
