//! SceneMain.deleteItemStart/Confirm and original delayed block0x86768.
use crate::{
    Scene, Track,
    construction::Bonus,
    production::{State, number},
    systems::Award,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
const PENDING: &str = "__native_obstacle_clears";
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub types: BTreeMap<u32, Vec<Bonus>>,
    pub tractor: Track,
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.types.is_empty()
            || self.types.len() > 128
            || self.types.iter().any(|(id, bs)| {
                !(8000..8100).contains(id)
                    || bs.len() != 2
                    || bs[0].kind != "experience"
                    || bs[1].kind != "playMoney"
                    || bs.iter().any(|b| {
                        b.count < 0
                            || b.count >= i32::MAX as i64
                            || !(0..=100).contains(&b.probability)
                    })
            })
        {
            return Err("Invalid obstacle clearing rules".into());
        }
        let t = &self.tractor;
        let image = scene
            .textures
            .get(&t.texture)
            .ok_or("Missing tractor atlas")?;
        if !t.fps.is_finite()
            || t.fps <= 0.
            || t.fps > 240.
            || t.frames.is_empty()
            || t.frames.len() > 2000
            || t.frames.iter().any(|f| {
                f.len() > 128
                    || f.iter().any(|p| {
                        p.src[0]
                            .checked_add(p.src[2])
                            .is_none_or(|n| n > image.width)
                            || p.src[1]
                                .checked_add(p.src[3])
                                .is_none_or(|n| n > image.height)
                            || p.rect.iter().any(|n| !n.is_finite())
                            || p.rect[2] <= 0.
                            || p.rect[3] <= 0.
                    })
            })
        {
            return Err("Invalid tractor animation bounds".into());
        }
        Ok(())
    }
}
impl State {
    pub fn pending_obstacle_clears(&self) -> Result<BTreeMap<String, i64>, String> {
        match self
            .systems
            .as_ref()
            .and_then(|s| s.preferences.get(PENDING))
        {
            None => Ok(BTreeMap::new()),
            Some(v) => {
                serde_json::from_value(v.clone()).map_err(|_| "Invalid pending clearings".into())
            }
        }
    }
    fn save_pending_clears(&mut self, items: &BTreeMap<String, i64>) -> Result<(), String> {
        let p = &mut self
            .systems
            .as_mut()
            .ok_or("Missing preferences")?
            .preferences;
        if items.is_empty() {
            p.remove(PENDING);
        } else {
            p.insert(
                PENDING.into(),
                serde_json::to_value(items).map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }
    pub fn can_clear_obstacle(&self, scene: &Scene, uid: &str) -> Result<(), String> {
        let v = self.entities.get(uid).ok_or("Препятствие не найдено")?;
        if self.mission.is_some() && !self.mission_paused()
            || v["isDeleted"] == true
            || v["isInBattle"] == true
            || !scene
                .obstacles
                .as_ref()
                .is_some_and(|c| c.types.contains_key(&(number(v, "type") as u32)))
        {
            return Err("Это препятствие сейчас нельзя убрать".into());
        }
        if !crate::expansions::owns_point(
            self,
            (number(v, "x") - 16) as i32,
            (number(v, "y") - 16) as i32,
        ) {
            return Err("Сначала купи этот участок".into());
        }
        Ok(())
    }
    pub fn clear_obstacle(
        &self,
        scene: &Scene,
        uid: &str,
        wall: i64,
    ) -> Result<(Self, String), String> {
        self.can_clear_obstacle(scene, uid)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let id = number(&next.entities[uid], "type") as u32;
        next.entities.get_mut(uid).unwrap()["isDeleted"] = true.into();
        if next.selected.as_deref() == Some(uid) {
            next.selected = None;
        }
        let mut pending = next.pending_obstacle_clears()?;
        pending.insert(uid.into(), now);
        next.save_pending_clears(&pending)?;
        let level = next.resources["level"];
        let systems = next.systems.as_mut().ok_or("Missing quest state")?;
        systems.achievement("remove_obstacles", 1, false);
        if let Some(c) = &scene.progression {
            systems.event(c, level, 30, 99999)?;
        }
        next.building_receipt(now, uid, id, "obstacle_clear", BTreeMap::new(), vec![])?;
        next.validate(scene)?;
        Ok((next, "Расчистка началась".into()))
    }
    pub fn obstacles_due(&self, scene: &Scene, wall: i64) -> Result<bool, String> {
        if scene.obstacles.is_none() {
            return Ok(false);
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        Ok(self
            .pending_obstacle_clears()?
            .values()
            .any(|at| now >= at + 5))
    }
    pub fn settle_obstacles(
        &self,
        scene: &Scene,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<Option<Self>, String> {
        if !self.obstacles_due(scene, wall)? {
            return Ok(None);
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let mut pending = self.pending_obstacle_clears()?;
        let due: Vec<_> = pending
            .iter()
            .filter(|(_, at)| now >= **at + 5)
            .map(|(uid, _)| uid.clone())
            .collect();
        for uid in due {
            let id = number(&next.entities[&uid], "type") as u32;
            let mut gains = BTreeMap::new();
            let mut words = vec![];
            // Block0x86768 rolls XP first, then coins;100% still consumes a word.
            for bonus in &scene.obstacles.as_ref().unwrap().types[&id] {
                if bonus.probability > 0 {
                    let word = entropy()?;
                    words.push(word);
                    if ((word % 100) as i64) < bonus.probability {
                        next.award(
                            &Award {
                                kind: bonus.kind.clone(),
                                count: bonus.count.to_string(),
                                item_id: 0,
                            },
                            scene,
                            now,
                            &mut gains,
                        )?;
                    }
                }
            }
            next.update_level(scene, now, &mut gains)?;
            pending.remove(&uid);
            next.save_pending_clears(&pending)?;
            next.building_receipt(now, &uid, id, "obstacle_bonus", gains, words)?;
        }
        next.validate(scene)?;
        Ok(Some(next))
    }
    pub fn validate_obstacles(&self, scene: &Scene) -> Result<(), String> {
        let Some(c) = &scene.obstacles else {
            return Ok(());
        };
        let pending = self.pending_obstacle_clears()?;
        if pending.len() > 1024 {
            return Err("Too many pending clearings".into());
        }
        let mut expected = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for seed in scene.entities.iter().filter(|e| e.kind == "obstacles") {
            let mut original = seed.archived.clone();
            let starts: Vec<_> = self
                .actions
                .iter()
                .filter(|a| a.uid == seed.uid && a.kind == "obstacle_clear")
                .collect();
            let bonuses: Vec<_> = self
                .actions
                .iter()
                .filter(|a| a.uid == seed.uid && a.kind == "obstacle_bonus")
                .collect();
            if !starts.is_empty() {
                if starts.len() != 1 || bonuses.len() > 1 || original["isDeleted"] == true {
                    return Err("Repeated obstacle clearing".into());
                }
                let a = starts[0];
                if a.recipe != seed.type_id
                    || !a.gains.is_empty()
                    || !a.entropy.is_empty()
                    || a.source.is_some()
                    || a.shop.is_some()
                    || !a.quest_events_pending.is_empty()
                {
                    return Err("Invalid clearing receipt".into());
                }
                original["isDeleted"] = true.into();
                seen.insert(seed.uid.clone());
                if let Some(b) = bonuses.first() {
                    if b.recipe != seed.type_id
                        || b.at < a.at + 5
                        || b.sequence <= a.sequence
                        || b.entropy.len()
                            != c.types[&seed.type_id]
                                .iter()
                                .filter(|b| b.probability > 0)
                                .count()
                    {
                        return Err("Invalid clearing reward order".into());
                    }
                    let mut words = b.entropy.iter();
                    let mut coin = 0;
                    let mut xp = 0;
                    for rule in &c.types[&seed.type_id] {
                        if rule.probability > 0
                            && ((**words.next().as_ref().unwrap() % 100) as i64) < rule.probability
                        {
                            if rule.kind == "playMoney" {
                                coin += rule.count;
                            } else {
                                xp += rule.count;
                            }
                        }
                    }
                    if b.gains.get("playMoney").copied().unwrap_or(0) != coin
                        || ![0, xp].contains(&b.gains.get("exp").copied().unwrap_or(0))
                        || b.source.is_some()
                        || b.shop.is_some()
                        || !b.quest_events_pending.is_empty()
                        || b.gains
                            .keys()
                            .any(|k| !["exp", "playMoney", "realMoney"].contains(&k.as_str()))
                    {
                        return Err("Invalid obstacle random reward".into());
                    }
                } else {
                    expected.insert(seed.uid.clone(), a.at);
                }
            } else if !bonuses.is_empty() {
                return Err("Reward without obstacle clearing".into());
            }
            if self.entities[&seed.uid] != original {
                return Err("Obstacle archive differs from source and clearing receipt".into());
            }
        }
        if pending != expected
            || self
                .actions
                .iter()
                .any(|a| a.kind.starts_with("obstacle_") && !seen.contains(&a.uid))
        {
            return Err("Invalid saved clearing queue".into());
        }
        Ok(())
    }
}

/// SPAnimation starts at its creation time, independently of the wall-clock
/// second used by the5s completion callback. Keep that phase ephemeral.
#[derive(Default)]
pub struct Playback {
    starts: std::cell::RefCell<BTreeMap<String, (i64, f64)>>,
}
impl Playback {
    pub fn elapsed(&self, uid: &str, at: i64, now: i64, seconds: f64) -> f64 {
        let mut starts = self.starts.borrow_mut();
        starts.retain(|_, (at, _)| now < at.saturating_add(5));
        let entry = starts
            .entry(uid.into())
            .or_insert((at, seconds - (now - at).max(0) as f64));
        if entry.0 != at {
            *entry = (at, seconds);
        }
        (seconds - entry.1).max(0.)
    }
}
/// Display-only detached source sprite and tractor; they never occupy/pick cells.
pub fn effects(
    view: &mut crate::interface::View,
    scene: &Scene,
    state: &State,
    camera: &crate::Camera,
    now: i64,
    seconds: f64,
    playback: Option<&Playback>,
) {
    let Some(c) = &scene.obstacles else {
        return;
    };
    let Some(template) = scene
        .interface
        .as_ref()
        .and_then(|c| c.symbols.get("main"))
        .and_then(|s| s.nodes.iter().find(|n| n.kind == "image"))
    else {
        return;
    };
    if state.mission.is_some() && !state.mission_paused() {
        return;
    }
    let Ok(pending) = state.pending_obstacle_clears() else {
        return;
    };
    for (uid, at) in pending {
        if now < at || now >= at + 5 {
            continue;
        }
        let Some(entity) = state.entities.get(&uid) else {
            continue;
        };
        let id = number(entity, "type") as u32;
        let tracks = scene
            .entities
            .iter()
            .find(|e| e.uid == uid)
            .map(|e| e.tracks.as_slice())
            .or_else(|| {
                scene
                    .world
                    .as_ref()?
                    .catalog
                    .get(&id)?
                    .variants
                    .get(&(number(entity, "direction") as u32))
                    .map(Vec::as_slice)
            });
        let Some(tracks) = tracks else { continue };
        let anchor = camera.screen(scene.projection.project(
            (number(entity, "x") - 16) as f64,
            (number(entity, "y") - 16) as f64,
        ));
        let elapsed = playback.map_or((now - at) as f64, |p| p.elapsed(&uid, at, now, seconds));
        for (name, tracks) in [
            ("clearing_obstacle", tracks),
            ("clearing_tractor", std::slice::from_ref(&c.tractor)),
        ] {
            for t in tracks {
                for p in t.frame(elapsed) {
                    let mut n = template.clone();
                    n.id = format!("{name}:{uid}");
                    n.parents.clear();
                    n.group.clear();
                    n.kind = "image".into();
                    n.texture = t.texture.clone();
                    n.text.clear();
                    // Retained UI images support full textures; the tractor uses a
                    // trimmed atlas, supplied as a cropped symbolic draw below.
                    n.rect = [
                        anchor[0] + p.rect[0] * camera.zoom,
                        anchor[1] + p.rect[1] * camera.zoom,
                        p.rect[2] * camera.zoom,
                        p.rect[3] * camera.zoom,
                    ];
                    n.source = Some(p.src);
                    view.draws.push(crate::interface::Draw {
                        node: n,
                        clip: None,
                        progress: 1.,
                        opacity: 255,
                        grayscale: false,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod visual_review94_checks {
    #[test]
    fn tractor_clock_starts_at_creation_phase_and_does_not_follow_wall_fraction() {
        let p = super::Playback::default();
        assert_eq!(p.elapsed("tree", 100, 100, 17.72), 0.);
        assert!((p.elapsed("tree", 100, 100, 17.82) - 0.1).abs() < 1e-9);
        assert!((p.elapsed("tree", 100, 101, 18.82) - 1.1).abs() < 1e-9);
        assert_eq!(p.elapsed("tree", 105, 105, 22.9), 0.);
    }
}
