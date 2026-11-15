//! Durable source callback precision. Legacy receipts keep their integer clock.
use crate::{
    Scene,
    production::{Action, State},
};
#[derive(Clone, Copy)]
pub(crate) struct CallbackTime {
    pub wall: i64,
    pub now_ms: i64,
    pub precise: bool,
}
impl CallbackTime {
    pub fn seconds(state: &State, scene: &Scene, wall: i64) -> Result<Self, String> {
        let now = state.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        Ok(Self {
            wall,
            now_ms: now * 1000,
            precise: false,
        })
    }
    pub fn millis(state: &State, scene: &Scene, wall_ms: i64) -> Result<Self, String> {
        if wall_ms <= 0 {
            return Err("Invalid callback clock".into());
        }
        let wall = wall_ms / 1000;
        let seconds = Self::seconds(state, scene, wall)?;
        let now_ms = seconds
            .now_ms
            .checked_add(wall_ms % 1000)
            .ok_or("Callback clock overflow")?;
        if now_ms >= (i32::MAX as i64) * 1000 {
            return Err("Callback clock out of range".into());
        }
        Ok(Self {
            wall,
            now_ms,
            precise: true,
        })
    }
    pub fn stamp(self, precise: bool) -> Option<i64> {
        precise.then_some(self.now_ms)
    }
    pub fn check_order(self, state: &State) -> Result<(), String> {
        if self.now_ms < state.last_action_time * 1000
            || state
                .actions
                .iter()
                .rev()
                .find_map(|a| a.at_ms)
                .is_some_and(|t| self.now_ms < t)
        {
            return Err("Часы переведены назад".into());
        }
        Ok(())
    }
}
pub(crate) fn time(a: &Action) -> i64 {
    a.at_ms.unwrap_or(a.at * 1000)
}
pub(crate) fn flight_delay(a: &Action) -> i64 {
    if a.at_ms.is_some() { 3750 } else { 4000 }
}
impl State {
    pub fn callback_game_ms(&self, scene: &Scene, wall_ms: i64) -> Result<i64, String> {
        Ok(CallbackTime::millis(self, scene, wall_ms)?.now_ms)
    }

    pub(crate) fn validate_callback_times(&self) -> Result<(), String> {
        let mut previous = 0;
        for a in &self.actions {
            if let Some(t) = a.at_ms {
                if t <= 0
                    || t >= i32::MAX as i64 * 1000
                    || t / 1000 != a.at
                    || t < previous
                    || !matches!(
                        a.kind.as_str(),
                        "gather_resource"
                            | "gather_material"
                            | "gather_research"
                            | "gather_upgrade_flight"
                            | "collect"
                            | "workshop_collect"
                            | "research_finish"
                            | "upgrade_finish"
                            | "flight_upgrade"
                            | "flight_upgrade_touch"
                            | "flight_research"
                            | "flight_research_touch"
                    )
                {
                    return Err("Invalid subsecond callback receipt".into());
                }
                previous = t;
            }
        }
        Ok(())
    }
}
