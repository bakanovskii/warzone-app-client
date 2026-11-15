//! Source Building.updateTooltipGatherUsingJuggler0xbf48d: two-second collection.
//! Integer-clock backend receipts retain their legacy six-second boundary.
//! New UI actions reserve storage and mark quests at touch; RNG/wallet at callback.
//! Legacy backend receipts keep their established immediate-commit semantics.
use crate::callback_clock::{CallbackTime, flight_delay, time};
use crate::{
    Scene,
    production::{Command, ProductionSource, State, number},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const KEY: &str = "__native_production_gathers";
pub const SECONDS: i64 = 6;
pub const SOURCE_SECONDS: i64 = 2;
fn seconds(precise: bool) -> i64 {
    if precise { SOURCE_SECONDS } else { SECONDS }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Family {
    Resource,
    Material,
    Research,
}
impl Family {
    fn begin(self) -> &'static str {
        match self {
            Self::Resource => "gather_resource",
            Self::Material => "gather_material",
            Self::Research => "gather_research",
        }
    }
    fn start(self) -> &'static str {
        match self {
            Self::Resource => "start",
            Self::Material => "workshop_start",
            Self::Research => "research_start",
        }
    }
    fn finish(self) -> &'static str {
        match self {
            Self::Resource => "collect",
            Self::Material => "workshop_collect",
            Self::Research => "research_finish",
        }
    }
    fn from_begin(kind: &str) -> Option<Self> {
        [Self::Resource, Self::Material, Self::Research]
            .into_iter()
            .find(|f| f.begin() == kind)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Pending {
    pub family: Family,
    pub recipe: u32,
    pub started: i64,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    pub sequence: u64,
}
impl State {
    pub fn pending_production_gathers(&self) -> Result<BTreeMap<String, Pending>, String> {
        match self.systems.as_ref().and_then(|s| s.preferences.get(KEY)) {
            None => Ok(BTreeMap::new()),
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|_| "Invalid production gather queue".into()),
        }
    }
    pub fn production_gathering(&self, uid: &str) -> bool {
        self.systems
            .as_ref()
            .and_then(|s| s.preferences.get(KEY))
            .is_some_and(|v| v.get(uid).is_some())
    }
    pub fn gathering(&self, uid: &str) -> bool {
        self.upgrade_gathering(uid) || self.production_gathering(uid)
    }
    fn save_production_gathers(&mut self, queue: &BTreeMap<String, Pending>) -> Result<(), String> {
        let prefs = &mut self
            .systems
            .as_mut()
            .ok_or("Missing gather preferences")?
            .preferences;
        if queue.is_empty() {
            prefs.remove(KEY);
        } else {
            prefs.insert(
                KEY.into(),
                serde_json::to_value(queue).map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }
    fn gather_duration(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        family: Family,
        started: i64,
        sequence: u64,
    ) -> Result<i64, String> {
        let e = self.entities.get(uid).ok_or("Missing gather building")?;
        let kind = number(e, "type") as u32;
        match family {
            Family::Resource => {
                if [1026, 1028].contains(&kind) {
                    return Err("Use harvester, not crystal field".into());
                }
                let c = scene.economy.as_ref().ok_or("Missing resource catalog")?;
                if !c
                    .buildings
                    .get(&kind)
                    .is_some_and(|b| b.recipes.contains(&id))
                {
                    return Err("Wrong resource gather building".into());
                }
                c.recipes
                    .get(&id)
                    .map(|r| r.seconds)
                    .ok_or("Unknown gathered resource".into())
            }
            Family::Material => {
                let c = scene.workshop.as_ref().ok_or("Missing workshop catalog")?;
                if !c.buildings.get(&kind).is_some_and(|ids| ids.contains(&id)) {
                    return Err("Wrong material gather building".into());
                }
                let r = c
                    .recipes
                    .get(&id)
                    .filter(|r| !r.unit())
                    .ok_or("Training requires placement, not gathering")?;
                let origin = self.workshop_origin_sequence(uid, id, started, sequence);
                Ok(self.upgraded_recipe(scene, r, origin).seconds)
            }
            Family::Research => {
                if kind != 1020 {
                    return Err("Wrong research gather building".into());
                }
                scene
                    .research
                    .as_ref()
                    .and_then(|c| c.items.get(&id))
                    .map(|r| r.seconds)
                    .ok_or("Unknown gathered research".into())
            }
        }
    }
    pub fn reserved_gather_resources(
        &self,
        scene: &Scene,
    ) -> Result<BTreeMap<String, i64>, String> {
        let mut reserved = BTreeMap::new();
        for p in self
            .pending_production_gathers()?
            .values()
            .filter(|p| p.family == Family::Resource)
        {
            let r = scene
                .economy
                .as_ref()
                .and_then(|c| c.recipes.get(&p.recipe))
                .ok_or("Unknown reserved resource recipe")?;
            for key in ["fuel", "iron"] {
                if let Some(out) = r.outputs.get(key) {
                    let n = reserved.entry(key.into()).or_insert(0i64);
                    *n = n.checked_add(out.count).ok_or("Gather storage overflow")?;
                }
            }
        }
        Ok(reserved)
    }
    pub fn begin_production_gather(
        &self,
        scene: &Scene,
        cmd: &Command,
        family: Family,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.begin_production_gather_at(scene, cmd, family, clock)
    }
    pub fn begin_production_gather_ms(
        &self,
        scene: &Scene,
        cmd: &Command,
        family: Family,
        wall_ms: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.begin_production_gather_at(scene, cmd, family, clock)
    }
    fn begin_production_gather_at(
        &self,
        scene: &Scene,
        cmd: &Command,
        family: Family,
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
            return Err("Gather requires ready production".into());
        };
        if self.gathering(uid) {
            return Err("Производство уже собирается".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time
            || now
                .checked_add(seconds(clock.precise))
                .is_none_or(|n| n >= i32::MAX as i64)
        {
            return Err("Недопустимое время сбора".into());
        }
        self.gather_duration(scene, uid, *recipe, family, *started, u64::MAX)?;
        // Eligibility validation on a discarded candidate. No real entropy provider
        // is accepted/called at touch, and no rolled result is saved or reused.
        match family {
            Family::Resource => {
                self.apply(scene, cmd, wall, || Ok(99))?;
            }
            Family::Material => {
                self.workshop_apply(scene, cmd, wall, || Ok(99))?;
            }
            Family::Research => {
                self.research_apply(scene, cmd, wall)?;
            }
        }
        if family == Family::Resource {
            let ec = scene.economy.as_ref().unwrap();
            let reserved = self.reserved_gather_resources(scene)?;
            for key in ["fuel", "iron"] {
                let additional = ec.recipes[recipe].outputs.get(key).map_or(0, |o| o.count);
                if additional == 0 {
                    continue;
                }
                if self.resources[key]
                    .checked_add(*reserved.get(key).unwrap_or(&0))
                    .and_then(|n| n.checked_add(additional))
                    .is_none_or(|n| n > self.capacity(ec, key))
                {
                    return Err("Склад занят уже собираемыми ресурсами".into());
                }
            }
        }
        let mut next = self.clone();
        next.selected = None;
        if family != Family::Research {
            if let Some(c) = &scene.progression {
                let s = next.systems.as_mut().ok_or("Missing quest state")?;
                s.event(c, self.resources["level"], 4, *recipe)?;
                s.event(c, self.resources["level"], 12, 99999)?;
            }
        }
        next.building_receipt(now, uid, *recipe, family.begin(), BTreeMap::new(), vec![])?;
        next.actions.last_mut().unwrap().at_ms = clock.stamp(clock.precise);
        next.actions.last_mut().unwrap().source = Some(ProductionSource {
            uid: uid.clone(),
            started: *started,
        });
        let mut queue = self.pending_production_gathers()?;
        queue.insert(
            uid.clone(),
            Pending {
                family,
                recipe: *recipe,
                started: *started,
                at: now,
                at_ms: clock.stamp(clock.precise),
                sequence: next.revision,
            },
        );
        next.save_production_gathers(&queue)?;
        next.validate(scene)?;
        Ok((next, "Собираем".into()))
    }
    pub fn production_gathers_due(&self, scene: &Scene, wall: i64) -> Result<bool, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.production_gathers_due_at(scene, clock)
    }
    pub fn production_gathers_due_ms(&self, scene: &Scene, wall_ms: i64) -> Result<bool, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.production_gathers_due_at(scene, clock)
    }
    fn production_gathers_due_at(
        &self,
        _scene: &Scene,
        clock: CallbackTime,
    ) -> Result<bool, String> {
        Ok(self.pending_production_gathers()?.values().any(|p| {
            clock.now_ms >= p.at_ms.unwrap_or(p.at * 1000) + seconds(p.at_ms.is_some()) * 1000
        }))
    }
    pub fn settle_production_gathers(
        &self,
        scene: &Scene,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<Option<Self>, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.settle_production_gathers_at(scene, clock, entropy)
    }
    pub fn settle_production_gathers_ms(
        &self,
        scene: &Scene,
        wall_ms: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<Option<Self>, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.settle_production_gathers_at(scene, clock, entropy)
    }
    fn settle_production_gathers_at(
        &self,
        scene: &Scene,
        clock: CallbackTime,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<Option<Self>, String> {
        let wall = clock.wall;
        if !self.production_gathers_due_at(scene, clock)? {
            return Ok(None);
        }
        self.validate(scene)?;
        clock.check_order(self)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let mut queue = self.pending_production_gathers()?;
        let mut due: Vec<_> = queue
            .iter()
            .filter(|(_, p)| {
                clock.now_ms >= p.at_ms.unwrap_or(p.at * 1000) + seconds(p.at_ms.is_some()) * 1000
            })
            .map(|(u, p)| (u.clone(), p.clone()))
            .collect();
        due.sort_by_key(|(_, p)| p.sequence);
        for (uid, p) in due {
            queue.remove(&uid);
            next.save_production_gathers(&queue)?;
            let cmd = Command::Collect {
                uid,
                recipe: p.recipe,
                started: p.started,
            };
            next = match p.family {
                Family::Resource => {
                    next.resource_after_gather(
                        scene,
                        &cmd,
                        wall,
                        clock.stamp(p.at_ms.is_some()),
                        &mut entropy,
                    )?
                    .0
                }
                Family::Material => {
                    next.workshop_after_gather(
                        scene,
                        &cmd,
                        wall,
                        clock.stamp(p.at_ms.is_some()),
                        &mut entropy,
                    )?
                    .0
                }
                Family::Research => {
                    next.research_after_gather(scene, &cmd, wall, clock.stamp(p.at_ms.is_some()))?
                        .0
                }
            };
        }
        next.validate(scene)?;
        Ok(Some(next))
    }
    pub(crate) fn validate_production_gathers(&self, scene: &Scene) -> Result<(), String> {
        let mut expected = BTreeMap::<String, Pending>::new();
        for a in &self.actions {
            if let Some(family) = Family::from_begin(&a.kind) {
                let source = a
                    .source
                    .as_ref()
                    .ok_or("Gather missing production identity")?;
                if source.uid != a.uid
                    || source.started <= 0
                    || a.at <= 0
                    || a.at > self.last_action_time
                    || !a.gains.is_empty()
                    || !a.entropy.is_empty()
                    || a.shop.is_some()
                    || !a.quest_events_pending.is_empty()
                    || expected.contains_key(&a.uid)
                {
                    return Err("Invalid/repeated production gather".into());
                }
                let origin = self
                    .actions
                    .iter()
                    .rev()
                    .find(|s| {
                        s.sequence < a.sequence
                            && s.uid == a.uid
                            && s.recipe == a.recipe
                            && s.kind == family.start()
                    })
                    .map(|s| s.at)
                    .or_else(|| {
                        scene
                            .entities
                            .iter()
                            .find(|e| {
                                e.uid == a.uid
                                    && number(&e.archived, "productionType") == a.recipe as i64
                            })
                            .map(|e| number(&e.archived, "productionStartTime"))
                    })
                    .ok_or("Gather without production origin")?;
                if self.production_forced_start(&a.uid, a.recipe, origin, a.sequence)
                    != source.started
                {
                    return Err("Gather production timer changed".into());
                }
                let seconds = self.gather_duration(
                    scene,
                    &a.uid,
                    a.recipe,
                    family,
                    source.started,
                    a.sequence,
                )?;
                if a.at <= source.started + seconds {
                    return Err("Gather started before ready".into());
                }
                expected.insert(
                    a.uid.clone(),
                    Pending {
                        family,
                        recipe: a.recipe,
                        started: source.started,
                        at: a.at,
                        at_ms: a.at_ms,
                        sequence: a.sequence,
                    },
                );
            } else if let Some(p) = expected.get(&a.uid) {
                if matches!(a.kind.as_str(), "flight_research" | "flight_research_touch") {
                    continue;
                }
                if a.kind != p.family.finish()
                    || a.recipe != p.recipe
                    || time(a) < p.at_ms.unwrap_or(p.at * 1000) + seconds(p.at_ms.is_some()) * 1000
                    || a.at_ms.is_some() != p.at_ms.is_some()
                {
                    return Err("Busy gather has an invalid intervening action".into());
                }
                expected.remove(&a.uid);
            }
        }
        let pending = self.pending_production_gathers()?;
        if pending != expected {
            return Err("Production gather queue differs from history".into());
        }
        let mut flown = std::collections::BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| matches!(a.kind.as_str(), "flight_research" | "flight_research_touch"))
        {
            let finished = self
                .actions
                .iter()
                .find(|f| f.kind == "research_finish" && f.uid == a.uid && f.recipe == a.recipe)
                .ok_or("Research flight without finish")?;
            let gather = self
                .actions
                .iter()
                .find(|g| g.kind == "gather_research" && g.uid == a.uid && g.recipe == a.recipe)
                .ok_or("Research flight without gather")?;
            if !flown.insert(a.recipe)
                || a.sequence <= finished.sequence
                || finished.sequence <= gather.sequence
                || a.at > self.last_action_time
                || time(a)
                    < time(finished)
                        + if a.kind == "flight_research_touch" {
                            0
                        } else {
                            flight_delay(finished)
                        }
                || a.at_ms.is_some() != finished.at_ms.is_some()
                || !a.gains.is_empty()
                || !a.entropy.is_empty()
                || a.source.is_some()
                || a.shop.is_some()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid/repeated research flight event".into());
            }
        }
        for (uid, p) in pending {
            let e = self
                .entities
                .get(&uid)
                .ok_or("Missing pending gather entity")?;
            if number(e, "state") != 3
                || e["isDeleted"] == true
                || e["isInBattle"] == true
                || number(e, "productionType") != p.recipe as i64
                || number(e, "productionStartTime") != p.started
            {
                return Err("Pending gather entity changed".into());
            }
        }
        Ok(())
    }
    pub fn pending_research_flights(&self) -> Vec<&crate::production::Action> {
        self.actions
            .iter()
            .filter(|f| {
                f.kind == "research_finish"
                    && self.actions.iter().any(|g| {
                        g.kind == "gather_research"
                            && g.uid == f.uid
                            && g.recipe == f.recipe
                            && g.sequence < f.sequence
                    })
                    && !self.actions.iter().any(|a| {
                        matches!(a.kind.as_str(), "flight_research" | "flight_research_touch")
                            && a.recipe == f.recipe
                    })
            })
            .collect()
    }
    pub fn research_flights_due(&self, scene: &Scene, wall: i64) -> Result<bool, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.research_flights_due_at(scene, clock)
    }
    pub fn research_flights_due_ms(&self, scene: &Scene, wall_ms: i64) -> Result<bool, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.research_flights_due_at(scene, clock)
    }
    fn research_flights_due_at(&self, _scene: &Scene, clock: CallbackTime) -> Result<bool, String> {
        Ok(self
            .pending_research_flights()
            .iter()
            .any(|a| clock.now_ms >= time(a) + flight_delay(a)))
    }
    pub fn settle_research_flights(
        &self,
        scene: &Scene,
        wall: i64,
    ) -> Result<Option<Self>, String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.settle_research_flights_at(scene, clock)
    }
    pub fn settle_research_flights_ms(
        &self,
        scene: &Scene,
        wall_ms: i64,
    ) -> Result<Option<Self>, String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.settle_research_flights_at(scene, clock)
    }
    fn settle_research_flights_at(
        &self,
        scene: &Scene,
        clock: CallbackTime,
    ) -> Result<Option<Self>, String> {
        if !self.research_flights_due_at(scene, clock)? {
            return Ok(None);
        }
        clock.check_order(self)?;
        let ids: Vec<_> = self
            .pending_research_flights()
            .iter()
            .filter(|a| clock.now_ms >= time(a) + flight_delay(a))
            .map(|a| a.recipe)
            .collect();
        let mut next = self.clone();
        for id in ids {
            next = next.finish_research_flight(scene, id, clock, false)?.0;
        }
        Ok(Some(next))
    }
    pub fn pickup_reward_bonus(
        &self,
        scene: &Scene,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::seconds(self, scene, wall)?;
        self.pickup_reward_bonus_at(scene, id, clock)
    }
    pub fn pickup_reward_bonus_ms(
        &self,
        scene: &Scene,
        id: u32,
        wall_ms: i64,
    ) -> Result<(Self, String), String> {
        let clock = CallbackTime::millis(self, scene, wall_ms)?;
        self.pickup_reward_bonus_at(scene, id, clock)
    }
    fn pickup_reward_bonus_at(
        &self,
        scene: &Scene,
        id: u32,
        clock: CallbackTime,
    ) -> Result<(Self, String), String> {
        if (11000..11200).contains(&id) {
            self.pickup_upgrade_bonus_at(scene, id, clock)
        } else {
            self.finish_research_flight(scene, id, clock, true)
        }
    }
    fn finish_research_flight(
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
            .pending_research_flights()
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
                52,
                id,
            )?;
        }
        next.building_receipt(
            now,
            &source.uid,
            id,
            if touch {
                "flight_research_touch"
            } else {
                "flight_research"
            },
            BTreeMap::new(),
            vec![],
        )?;
        next.actions.last_mut().unwrap().at_ms = clock.stamp(source.at_ms.is_some());
        next.validate(scene)?;
        Ok((next, "Бонус исследования получен".into()))
    }
}
