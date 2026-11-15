//! Bounded, cosmetic playback of an already committed turn. Never changes rules,
//! RNG, saves or receipts. New commands append object-local queue rows while
//! existing movement, attacks and enemy responses finish playing.
use crate::{
    Entity, Scene, Track,
    battle::{Config, VisualEvent},
};
use std::collections::BTreeMap;

const TILE_SECONDS: f64 = 1. / (1.5 * 1.32);
#[derive(Clone, Debug)]
enum Kind {
    Move(Vec<[i32; 2]>),
    Attack(String, f64),
    Health { from: f64, to: i32 },
    Death,
}
#[derive(Clone, Debug)]
struct Stage {
    start: f64,
    duration: f64,
    kind: Kind,
}
#[derive(Clone, Debug)]
struct Actor {
    seed: Entity,
    stages: Vec<Stage>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SoundCue {
    pub at: f64,
    pub type_id: u32,
    pub event: &'static str,
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub target: String,
    pub clip: String,
    pub start: f64,
    pub end: f64,
    pub attack_end: f64,
    pub priority: u32,
}
pub struct Playback {
    actors: BTreeMap<String, Actor>,
    pub duration: f64,
    pub ghosts: Vec<Entity>,
    pub sounds: Vec<SoundCue>,
    pub hits: Vec<Hit>,
    barriers: Vec<Barrier>,
}
#[derive(Clone)]
struct Barrier {
    kind: u8,
    unit: Option<String>,
    target: Option<String>,
    start: f64,
    end: f64,
}
pub struct Pose<'a> {
    pub x: f64,
    pub y: f64,
    pub hp: f64,
    pub facing: String,
    pub pose: &'static str,
    pub time: f64,
    pub original: &'a Entity,
    pub finished_death: bool,
}

// SceneBattle.queueUpdate0x553f1 examines every queued row on each frame.
// Its four previous-action predicates are object-local, not a global barrier.
// Death rows store the killer in pUnit and the victim in pFireTarget
// (queueAddDeath:from:0x5473d); treating the victim as pUnit serializes unrelated
// squads and also changes when friendly kill/death voices are admitted.
struct QueueRow<'a> {
    index: usize,
    kind: u8,
    unit: Option<&'a str>,
    target: Option<&'a str>,
    start: f64,
    end: f64,
}
impl QueueRow<'_> {
    fn blocks_unit(&self, unit: &str) -> bool {
        matches!(self.kind, 1 | 2) && self.unit == Some(unit)
            || matches!(self.kind, 2 | 3) && self.target == Some(unit)
    }
    fn blocks_target(&self, target: &str) -> bool {
        matches!(self.kind, 1 | 2) && self.unit == Some(target)
            || self.kind == 3 && self.target == Some(target)
    }
}

fn killer<'a>(events: &'a [VisualEvent], index: usize, victim: &str) -> Option<&'a str> {
    events[..index].iter().rev().find_map(|event| match event {
        VisualEvent::Attack {
            uid,
            target,
            damage,
            ..
        } if target == victim && *damage > 0 => Some(uid.as_str()),
        _ => None,
    })
}
fn facing(a: [i32; 2], b: [i32; 2], old: &str) -> String {
    match ((b[0] - a[0]).signum(), (b[1] - a[1]).signum()) {
        (1, 0) => "se",
        (0, 1) => "sw",
        (0, -1) => "ne",
        (-1, 0) => "nw",
        (-1, -1) => "n",
        (1, 1) => "s",
        (1, -1) => "e",
        (-1, 1) => "w",
        _ => old,
    }
    .into()
}
fn direction(e: &Entity) -> String {
    crate::entity_state::direction_name(e.direction as i32).into()
}
fn key(c: &Config, e: &Entity, d: &str, pose: &str) -> String {
    if e.kind != "unit" {
        return format!("building/{}/{d}/{pose}", e.type_id);
    }
    format!(
        "{}/{}/{d}/{pose}",
        e.archived["visual_model"]
            .as_str()
            .unwrap_or(&c.defs[&e.type_id].model),
        if e.archived["enemy"] == true {
            "black"
        } else {
            "yellow"
        }
    )
}
fn duration(c: &Config, e: &Entity, d: &str, pose: &str) -> f64 {
    c.animations
        .get(&key(c, e, d, pose))
        .map(|ts| {
            ts.iter()
                .map(|t| t.frames.len() as f64 / t.fps.max(1.))
                .fold(0., f64::max)
        })
        .unwrap_or(0.35)
        .max(0.05)
}
impl Playback {
    pub fn snapshot(&self, elapsed: f64) -> serde_json::Value {
        serde_json::json!({"elapsed":elapsed,"duration":self.duration,"hits":self.hits.iter().filter(|h|h.start<=elapsed && elapsed<h.end).map(|h|serde_json::json!({"target":h.target,"clip":h.clip,"time":elapsed-h.start})).collect::<Vec<_>>(),"actors":self.actors.keys().filter_map(|uid|self.sample(uid,elapsed).map(|p|serde_json::json!({"uid":uid,"x":p.x,"y":p.y,"hp":p.hp,"pose":p.pose,"facing":p.facing,"finished_death":p.finished_death}))).collect::<Vec<_>>()})
    }
    pub fn new(before: &Scene, after: &Scene, c: &Config, events: &[VisualEvent]) -> Self {
        Self::scheduled(before, after, c, events, &[], 0.)
    }
    /// Keep active object-local rows when a different squad receives an order.
    /// Reusing the durable scene alone would drop its queued fire/death/audio.
    pub fn append(
        &mut self,
        before: &Scene,
        after: &Scene,
        c: &Config,
        events: &[VisualEvent],
        elapsed: f64,
    ) {
        let next = Self::scheduled(before, after, c, events, &self.barriers, elapsed);
        for (uid, actor) in next.actors {
            if self.actors.contains_key(&uid) {
                for mut stage in actor.stages {
                    // A later shot can arrive during an earlier health tween.
                    // Start from its visible value, keeping the already committed
                    // final HP, rather than snapping to the preceding turn's HP.
                    let visible = self.sample(&uid, stage.start).map(|p| p.hp);
                    if let Kind::Health { from, .. } = &mut stage.kind {
                        if let Some(hp) = visible {
                            *from = hp;
                        }
                    }
                    let old = self.actors.get_mut(&uid).unwrap();
                    old.stages.push(stage);
                    old.stages.sort_by(|a, b| a.start.total_cmp(&b.start));
                }
            } else {
                self.actors.insert(uid, actor);
            }
        }
        self.hits.extend(next.hits);
        self.sounds.extend(next.sounds);
        self.sounds.sort_by(|a, b| a.at.total_cmp(&b.at));
        self.barriers = next.barriers;
        self.duration = self.duration.max(next.duration);
        self.ghosts = self
            .actors
            .values()
            .filter(|a| !after.entities.iter().any(|e| e.uid == a.seed.uid))
            .map(|a| a.seed.clone())
            .collect();
    }
    fn scheduled(
        before: &Scene,
        after: &Scene,
        c: &Config,
        events: &[VisualEvent],
        previous: &[Barrier],
        offset: f64,
    ) -> Self {
        let mut out = Self {
            actors: BTreeMap::new(),
            duration: 0.,
            ghosts: vec![],
            sounds: vec![],
            hits: vec![],
            barriers: vec![],
        };
        let mut queue: Vec<QueueRow<'_>> = previous
            .iter()
            .filter(|r| r.end > offset)
            .map(|r| QueueRow {
                index: usize::MAX,
                kind: r.kind,
                unit: r.unit.as_deref(),
                target: r.target.as_deref(),
                start: r.start,
                end: r.end,
            })
            .collect();
        for (event_index, event) in events.iter().take(4096).enumerate() {
            let (uid, target) = match event {
                VisualEvent::Move { uid, .. } | VisualEvent::Death { uid } => (uid, None),
                VisualEvent::Attack { uid, target, .. } => (uid, Some(target)),
            };
            for id in std::iter::once(uid).chain(target) {
                if !out.actors.contains_key(id) {
                    if let Some(e) = before
                        .entities
                        .iter()
                        .chain(&after.entities)
                        .find(|e| &e.uid == id)
                    {
                        out.actors.insert(
                            id.clone(),
                            Actor {
                                seed: e.clone(),
                                stages: vec![],
                            },
                        );
                    }
                }
            }
            let Some(actor) = out.actors.get(uid) else {
                continue;
            };
            let (kind, unit, queue_target, time) = match event {
                VisualEvent::Move { uid, route } => (
                    1,
                    Some(uid.as_str()),
                    None,
                    route.len().saturating_sub(1) as f64 * TILE_SECONDS,
                ),
                VisualEvent::Attack {
                    uid,
                    target,
                    facing,
                    ..
                } => (
                    2,
                    Some(uid.as_str()),
                    Some(target.as_str()),
                    if [2000, 2001, 2018, 1021].contains(&actor.seed.type_id) {
                        2.
                    } else {
                        duration(c, &actor.seed, facing, "attack")
                    },
                ),
                VisualEvent::Death { uid } => (
                    3,
                    killer(events, event_index, uid),
                    Some(uid.as_str()),
                    duration(c, &actor.seed, &direction(&actor.seed), "death"),
                ),
            };
            let start = queue
                .iter()
                .filter(|row| {
                    unit.is_some_and(|unit| row.blocks_unit(unit))
                        || queue_target.is_some_and(|target| row.blocks_target(target))
                })
                .map(|row| row.end)
                .fold(offset, f64::max);
            queue.push(QueueRow {
                index: event_index,
                kind,
                unit,
                target: queue_target,
                start,
                end: start + time,
            });
        }
        // Queue insertion order and start-time order need not agree: a later
        // independent actor can start before an earlier blocked row. Build
        // health tweens and hit-priority replacement in actual playback order.
        queue.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.index.cmp(&b.index)));
        out.barriers = queue
            .iter()
            .map(|r| Barrier {
                kind: r.kind,
                unit: r.unit.map(str::to_owned),
                target: r.target.map(str::to_owned),
                start: r.start,
                end: r.end,
            })
            .collect();
        for row in queue {
            if row.index == usize::MAX {
                continue;
            }
            let event_index = row.index;
            let event = &events[event_index];
            let uid = match event {
                VisualEvent::Move { uid, .. }
                | VisualEvent::Attack { uid, .. }
                | VisualEvent::Death { uid } => uid,
            };
            let actor = &out.actors[uid];
            let (kind, time) = match event {
                VisualEvent::Move { route, .. } => (
                    Kind::Move(route.clone()),
                    route.len().saturating_sub(1) as f64 * TILE_SECONDS,
                ),
                VisualEvent::Attack { facing, .. } => (
                    Kind::Attack(facing.clone(), duration(c, &actor.seed, facing, "attack")),
                    if [2000, 2001, 2018, 1021].contains(&actor.seed.type_id) {
                        2.
                    } else {
                        duration(c, &actor.seed, facing, "attack")
                    },
                ),
                VisualEvent::Death { .. } => (
                    Kind::Death,
                    duration(c, &actor.seed, &direction(&actor.seed), "death"),
                ),
            };
            let start = row.start;
            let entity = &actor.seed;
            let friendly = entity.archived["enemy"] != true;
            let unit = (2000..3000).contains(&entity.type_id);
            let mut cue = |at, e: &Entity, event| {
                out.sounds.push(SoundCue {
                    at,
                    type_id: e.type_id,
                    event,
                })
            };
            match event {
                VisualEvent::Move { .. } if friendly && unit => cue(start, entity, "order"),
                VisualEvent::Attack { target, .. } => {
                    if friendly && unit {
                        cue(start, entity, "fire");
                    }
                    cue(start, entity, "shoot");
                    // queueRunAttack voices target damage at attack start.
                    if let Some(target) = before.entities.iter().find(|e| &e.uid == target) {
                        if (2000..3000).contains(&target.type_id)
                            && target.archived["enemy"] != true
                        {
                            cue(start, target, "damage");
                        }
                    }
                }
                VisualEvent::Death { uid } => {
                    if friendly && unit {
                        cue(start, entity, "death");
                    }
                    // queueRunDeath voices the surviving attacker after the shot.
                    if let Some(attacker) = events[..event_index]
                        .iter()
                        .rev()
                        .find_map(|e| {
                            if let VisualEvent::Attack {
                                uid: attacker,
                                target,
                                ..
                            } = e
                            {
                                (target == uid).then_some(attacker)
                            } else {
                                None
                            }
                        })
                        .and_then(|uid| before.entities.iter().find(|e| &e.uid == uid))
                    {
                        if (2000..3000).contains(&attacker.type_id)
                            && attacker.archived["enemy"] != true
                        {
                            cue(start, attacker, "kill");
                        }
                    }
                }
                _ => {}
            }
            // SceneBattle.queueRunAttack0x52e8d starts the victim's hit clip
            // at the start of the attack, using the attacker's endamageAnim.
            if let VisualEvent::Attack { target, .. } = event {
                if let Some(clip) = c.hit_types.get(&entity.type_id) {
                    if let Some(asset) = c.hit_clips.get(clip) {
                        let duration = asset
                            .tracks
                            .iter()
                            .map(|t| {
                                if t.fps > 0. {
                                    t.frames.len() as f64 / t.fps
                                } else {
                                    0.
                                }
                            })
                            .fold(0., f64::max);
                        let active = out
                            .hits
                            .iter_mut()
                            .rev()
                            .find(|h| h.target == *target && h.start <= start && start < h.end);
                        // Unit0xc77cd / Building0xc2ccd: equal or lower priority
                        // does not restart a playing clip. A stronger hit replaces it.
                        let blocked = active
                            .as_ref()
                            .is_some_and(|h| h.priority >= asset.priority);
                        if !blocked && duration > 0. {
                            if let Some(h) = active {
                                h.end = start;
                            }
                            out.hits.push(Hit {
                                target: target.clone(),
                                clip: clip.clone(),
                                start,
                                end: (start + duration).min(start + time),
                                attack_end: start + time,
                                priority: asset.priority,
                            });
                        }
                    }
                }
            }
            out.actors.get_mut(uid).unwrap().stages.push(Stage {
                start,
                duration: time,
                kind,
            });
            out.duration = out.duration.max(start + time);
            if let VisualEvent::Attack { target, damage, .. } = event {
                if *damage == 0 {
                    continue;
                } // Firing still plays; no second health tween.
                if let Some(a) = out.actors.get_mut(target) {
                    let hp = a
                        .stages
                        .iter()
                        .rev()
                        .find_map(|s| {
                            if let Kind::Health { to, .. } = s.kind {
                                Some(to)
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| a.seed.archived["hp"].as_i64().unwrap_or(0) as i32);
                    // Unit.updateProgressHealth:usingJuggler: uses a 1.5s
                    // linear width tween, starting with battleDoAttack.
                    let from = out.sample(target, start).map_or(hp as f64, |p| p.hp);
                    out.actors.get_mut(target).unwrap().stages.push(Stage {
                        start,
                        duration: 1.5,
                        kind: Kind::Health {
                            from,
                            to: hp - damage,
                        },
                    });
                }
            }
        }
        out.sounds.sort_by(|a, b| a.at.total_cmp(&b.at));
        out.duration = out.hits.iter().map(|h| h.end).fold(out.duration, f64::max);
        out.ghosts = out
            .actors
            .values()
            .filter(|a| !after.entities.iter().any(|e| e.uid == a.seed.uid))
            .map(|a| a.seed.clone())
            .collect();
        out
    }
    pub fn sample(&self, uid: &str, elapsed: f64) -> Option<Pose<'_>> {
        let a = self.actors.get(uid)?;
        let mut p = Pose {
            x: a.seed.x,
            y: a.seed.y,
            hp: a.seed.archived["hp"].as_f64().unwrap_or(0.),
            facing: direction(&a.seed),
            pose: "idle",
            time: elapsed,
            original: &a.seed,
            finished_death: false,
        };
        for s in &a.stages {
            if elapsed < s.start {
                break;
            }
            let local = (elapsed - s.start).max(0.);
            match &s.kind {
                Kind::Move(route) if route.len() > 1 => {
                    let t = (local / TILE_SECONDS).min((route.len() - 1) as f64);
                    let i = (t.floor() as usize).min(route.len() - 2);
                    let f = (t - i as f64).min(1.);
                    p.x = route[i][0] as f64 + (route[i + 1][0] - route[i][0]) as f64 * f - 16.;
                    p.y = route[i][1] as f64 + (route[i + 1][1] - route[i][1]) as f64 * f - 16.;
                    p.facing = facing(route[i], route[i + 1], &p.facing);
                    p.pose = if local < s.duration { "run" } else { "idle" };
                    p.time = local;
                }
                Kind::Attack(d, clip_duration) => {
                    p.facing = d.clone();
                    p.pose = if local < s.duration { "attack" } else { "idle" };
                    p.time = if local < s.duration {
                        local.min((clip_duration - 0.00001).max(0.))
                    } else {
                        local
                    };
                }
                Kind::Health { from, to } => {
                    p.hp = from + (*to as f64 - from) * (local / s.duration).clamp(0., 1.);
                }
                Kind::Death => {
                    p.pose = "death";
                    p.time = local.min((s.duration - 0.00001).max(0.));
                    p.hp = 0.;
                    p.finished_death = local >= s.duration;
                }
                _ => {}
            }
        }
        Some(p)
    }
    pub fn tracks<'a>(c: &'a Config, p: &Pose<'a>, current: &'a Entity) -> &'a [Track] {
        if current.kind != "unit" {
            return if p.finished_death {
                &current.tracks
            } else {
                &p.original.tracks
            };
        }
        if p.pose != "idle" {
            if let Some(t) = c.animations.get(&key(c, current, &p.facing, p.pose)) {
                return t;
            }
        }
        let idle = format!(
            "{}/{}/{}",
            current.archived["visual_model"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| current.type_id.to_string()),
            if current.archived["enemy"] == true {
                "black"
            } else {
                "yellow"
            },
            p.facing
        );
        c.sprites
            .get(&idle)
            .map(Vec::as_slice)
            .unwrap_or(&current.tracks)
    }
}
