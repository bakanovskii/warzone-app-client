//! Native mission rules, ported from recovery/battle.py and pathing.py.
//! Those references cite the original SceneBattle/Units/Triggers Thumb addresses.
//! No guest or server calls. Tutorial script/bonuses are outside this module.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[path = "battle_tutorial.rs"]
mod tutorial_impl;
pub use tutorial_impl::{Tutorial, TutorialCommand, TutorialStep};
#[path = "battle_pvp.rs"]
mod pvp_impl;
pub const GRID: i32 = 128;
pub const OFFSET: i32 = 16;
const WAVE: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, -1),
    (0, 1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
const BACK: [(i32, i32); 8] = [
    (0, -1),
    (0, 1),
    (-1, 0),
    (1, 0),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];
fn idx(x: i32, y: i32) -> usize {
    (x * GRID + y) as usize
}
fn inside(x: i32, y: i32) -> bool {
    (0..GRID).contains(&x) && (0..GRID).contains(&y)
}
fn cheb(a: [i32; 2], b: [i32; 2]) -> i32 {
    (a[0] - b[0]).abs().max((a[1] - b[1]).abs())
}
fn heavy(id: u32) -> bool {
    [2006, 2007, 2009, 2011, 2015].contains(&id)
}
fn through(v: i32, t: u32) -> bool {
    !heavy(v as u32) && !heavy(t) && (2000..3000).contains(&v)
}
/// Original repeated row-major sweeps and backtracking neighbour order.
/// Returns start..end, retaining the source's diagonal/corner-cutting rules.
pub fn trace(
    c: &[i32],
    start: [i32; 2],
    end: [i32; 2],
    dist: i32,
    t: u32,
) -> Option<Vec<[i32; 2]>> {
    if c.len() != 16384
        || !inside(start[0], start[1])
        || !end.iter().all(|n| (1..=126).contains(n))
        || !(0..=64).contains(&dist)
    {
        return None;
    }
    let m = distance_map(c, start, dist, t)?;
    let mut p = end;
    let mut out = vec![p];
    if m[idx(p[0], p[1])] == 0 {
        return None;
    }
    while m[idx(p[0], p[1])] != 1 {
        if out.len() > 128 || !p.iter().all(|v| (1..=126).contains(v)) {
            return None;
        }
        let mut best = 1024;
        let mut next = None;
        for (dx, dy) in BACK {
            let n = [p[0] + dx, p[1] + dy];
            let v = m[idx(n[0], n[1])];
            if v > 0 && v < best {
                best = v;
                next = Some(n);
            }
        }
        p = next?;
        out.push(p);
    }
    out.reverse();
    Some(out)
}
/// Same source sweeps as trace; shared by the moving-area overlay so selecting
/// a troop computes the field once instead of tracing every visible cell.
pub fn distance_map(c: &[i32], start: [i32; 2], dist: i32, t: u32) -> Option<Vec<i32>> {
    if c.len() != 16384 || !inside(start[0], start[1]) || !(0..=64).contains(&dist) {
        return None;
    }
    let mut m = vec![0i32; 16384];
    let mut done = vec![false; 16384];
    m[idx(start[0], start[1])] = 1;
    loop {
        let mut changed = false;
        for x in (start[0] - dist).max(1)..=(start[0] + dist).min(126) {
            for y in (start[1] - dist).max(1)..=(start[1] + dist).min(126) {
                let k = idx(x, y);
                let v = m[k];
                if v == 0 || done[k] {
                    continue;
                }
                done[k] = true;
                for (dx, dy) in WAVE {
                    let nk = idx(x + dx, y + dy);
                    if (c[nk] != 0 && !through(c[nk], t)) || done[nk] {
                        continue;
                    }
                    if m[nk] == 0 || m[nk] > v + 1 {
                        m[nk] = v + 1;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    Some(m)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub group: String,
    pub alias: String,
    pub model: String,
    pub width: i32,
    pub height: i32,
    pub health: i32,
    pub attack: i32,
    pub distance: i32,
    pub speed: i32,
    pub ability: i32,
    pub heavy: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeedEntity {
    pub uid: String,
    pub id: u32,
    pub x: f64,
    pub y: f64,
    #[serde(default = "se")]
    pub d: String,
    #[serde(default = "neutral")]
    pub fraction: String,
}
fn se() -> String {
    "se".into()
}
fn neutral() -> String {
    "neutral".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default, rename = "type")]
    pub kind: i32,
}
impl Zone {
    fn contains(&self, p: [i32; 2]) -> bool {
        self.x + OFFSET <= p[0]
            && p[0] < self.x + OFFSET + self.w
            && self.y + OFFSET <= p[1]
            && p[1] < self.y + OFFSET + self.h
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mission {
    #[serde(default)]
    pub no_bonuses: bool,
    #[serde(default)]
    pub map_position: [i32; 2],
    #[serde(default)]
    pub map_marker: String,
    #[serde(default)]
    pub unlock_price: i64,
    #[serde(default)]
    pub parent: u32,
    #[serde(default)]
    pub awards: Vec<crate::systems::Award>,
    #[serde(default = "one")]
    pub min_descent: usize,
    pub mission: String,
    pub mission_id: u32,
    pub title: String,
    pub entities: Vec<SeedEntity>,
    pub obstacles: Vec<SeedEntity>,
    pub zones: Vec<Zone>,
    pub triggers: Vec<Value>,
    pub fractions: Vec<String>,
    pub locked: Vec<Value>,
    pub fog: [i32; 4],
    pub map_data: Vec<[i32; 3]>,
}
fn one() -> usize {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HitClip {
    pub priority: u32,
    pub tracks: Vec<crate::Track>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub destroy_bonuses: BTreeMap<u32, Vec<crate::pvp::Drop>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pvp: Option<crate::pvp::Config>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<crate::battle_selection::Config>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tutorial_steps: Vec<TutorialStep>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hit_clips: BTreeMap<String, HitClip>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hit_types: BTreeMap<u32, String>,
    pub defs: BTreeMap<u32, Definition>,
    pub missions: BTreeMap<String, Mission>,
    #[serde(default)]
    pub views: BTreeMap<String, MissionView>,
    #[serde(default)]
    pub sprites: BTreeMap<String, Vec<crate::Track>>,
    #[serde(default)]
    pub animations: BTreeMap<String, Vec<crate::Track>>,
}
impl Config {
    pub fn campaign(&self) -> impl DoubleEndedIterator<Item = &Mission> {
        self.missions
            .values()
            .filter(|m| !matches!(m.mission_id, 5000 | 5990))
    }
    pub fn campaign_names(&self) -> impl DoubleEndedIterator<Item = &String> {
        self.missions
            .iter()
            .filter(|(_, m)| !matches!(m.mission_id, 5000 | 5990))
            .map(|(k, _)| k)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissionView {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ground_details: Vec<crate::TerrainSprite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_detail_layer: Option<usize>,
    pub terrain: Vec<crate::TerrainSprite>,
    pub camera: [f64; 3],
    pub sprites: BTreeMap<String, Vec<crate::Track>>,
    pub fences: BTreeMap<String, Vec<crate::Track>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArmyUnit {
    pub uid: String,
    pub type_id: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unit {
    pub uid: String,
    pub id: u32,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub direction: String,
    pub fraction: String,
    pub w: i32,
    pub h: i32,
    pub hp: i32,
    pub hp_max: i32,
    pub attack: i32,
    pub distance: i32,
    pub speed: i32,
    pub ability: i32,
    pub heavy: bool,
    pub enemy: bool,
    pub own: bool,
    pub mode: i32,
    pub zone: [i32; 4],
    pub target: Option<String>,
    pub appropriate: bool,
    pub damaged: bool,
    pub profile: Option<String>,
    pub pending: bool,
    pub by_trigger: bool,
}
impl Unit {
    /// SceneBattle distinguishes enemy troops by bIsEnemy, while every
    /// destructible Building (including fences tagged player in mission XML)
    /// can be targeted. The map fraction alone is not an ownership flag.
    pub fn player_target(&self) -> bool {
        !self.own
            && ((self.kind == "unit" && self.enemy)
                || matches!(self.kind.as_str(), "building" | "decor"))
    }
    pub fn cell(&self) -> [i32; 2] {
        [self.x as i32, self.y as i32]
    }
    fn size(&self) -> [i32; 2] {
        if ["se", "nw"].contains(&self.direction.as_str()) {
            [self.w, self.h]
        } else {
            [self.h, self.w]
        }
    }
    pub fn distance_to(&self, t: &Unit) -> i32 {
        let a = self.cell();
        if t.kind == "unit" || (t.w == 1 && t.h == 1) {
            return cheb(a, t.cell());
        }
        let [w, h] = t.size();
        let x = (t.x - (w / 2) as f64) as i32;
        let y = (t.y - (h / 2) as f64) as i32;
        [
            [x, y],
            [x, y + h - 1],
            [x + w - 1, y],
            [x + w - 1, y + h - 1],
        ]
        .iter()
        .map(|b| cheb(a, *b))
        .min()
        .unwrap()
    }
    pub fn range(&self) -> i32 {
        if self.ability == 10 {
            self.speed + 1
        } else {
            self.distance
        }
    }
    pub fn damage_to(&self, t: &Unit) -> i32 {
        let mut a = self.attack;
        if self.ability == 11 && t.kind == "unit" && !t.heavy && a != 0 {
            a *= 2;
        }
        if self.kind == "unit" && !self.heavy && t.kind == "unit" && t.ability == 7 {
            a = (a + 1) / 2;
        }
        a
    }
    fn can_attack(&self) -> bool {
        [1, 2, 4].contains(&self.mode) && (self.kind == "unit" || self.attack != 0)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trigger {
    pub name: String,
    pub on: bool,
    pub events: Vec<Value>,
    pub actions: Vec<Value>,
    pub done: Vec<bool>,
    pub conditions: Vec<Value>,
}
/// Persisted only for missions opened with original story presentation. Old
/// saves omit this whole field and keep their original checksum representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoryFlow {
    pub message: Option<StoryMessage>,
    tasks: VecDeque<StoryTask>,
    #[serde(skip)]
    pumping: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoryMessage {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub canonical_phrases: bool,
    pub mask: String,
    pub phrase: usize,
    trigger: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum StoryTask {
    Trigger(usize),
    Event {
        code: i32,
        data: Value,
        trigger: usize,
        event: usize,
    },
    Results(usize),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Queued {
    Move {
        u: usize,
        end: [i32; 2],
        start: Option<[i32; 2]>,
        route: Option<Vec<[i32; 2]>>,
    },
    Attack(usize, usize),
    Death(usize, usize),
}
/// Committed turn presentation only. Excluded from saves/checksums: a restart
/// resumes the durable final state rather than replaying an old turn or reward.
#[derive(Clone, Debug)]
pub enum VisualEvent {
    Move {
        uid: String,
        route: Vec<[i32; 2]>,
    },
    Attack {
        uid: String,
        target: String,
        damage: i32,
        facing: String,
    },
    Death {
        uid: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Battle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pvp: Option<crate::pvp::Arena>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutorial: Option<Tutorial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_request: Option<crate::battle_camera::Request>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<StoryFlow>,
    pub mission: String,
    pub phase: String,
    pub result: Option<String>,
    pub energy: i32,
    pub units: Vec<Unit>,
    pub roster: Vec<ArmyUnit>,
    pub triggers: Vec<Trigger>,
    pub log: Vec<String>,
    queue: VecDeque<Queued>,
    pending_deaths: BTreeSet<usize>,
    route_shortest: Vec<[i32; 2]>,
    #[serde(skip)]
    pub visuals: Vec<VisualEvent>,
}
impl Battle {
    pub fn new(c: &Config, name: &str, roster: Vec<ArmyUnit>) -> Result<Self, String> {
        let m = c.missions.get(name).ok_or("Unknown mission")?;
        if name == "tutorial_mission" && c.tutorial_steps.len() != 27 {
            return Err(
                "Tutorial choreography is not supported in the normal mission engine".into(),
            );
        }
        let mut b = Self {
            pvp: (name == "pvptutorial_1").then(crate::pvp::Arena::empty),
            tutorial: (name == "tutorial_mission").then(Tutorial::default),
            camera_request: None,
            story: None,
            mission: name.into(),
            phase: "placement".into(),
            result: None,
            energy: 0,
            units: vec![],
            roster,
            triggers: vec![],
            log: vec![],
            queue: VecDeque::new(),
            pending_deaths: BTreeSet::new(),
            route_shortest: vec![],
            visuals: vec![],
        };
        for e in m.entities.iter().chain(&m.obstacles) {
            b.add(c, e, false, name != "pvptutorial_1")?;
        }
        for t in &m.triggers {
            let mut a = vec![];
            for v in t["actions"].as_array().ok_or("Invalid mission actions")? {
                let name = v["name"].as_str().unwrap_or("");
                if ![
                    "MessageBoxAction",
                    "CloseMissionAction",
                    "SpawnEntityAction",
                    "ShowEntityAction",
                    "SetTargetForIA",
                    "Complete_Objective",
                    "MoveCameraAction",
                    "ActiveTriggerAction",
                ]
                .contains(&name)
                {
                    continue;
                }
                if name == "MessageBoxAction" {
                    a.insert(0, v.clone());
                } else {
                    a.push(v.clone());
                }
            }
            b.triggers.push(Trigger {
                name: t["name"].as_str().unwrap_or("").into(),
                on: true,
                events: t["events"].as_array().cloned().unwrap_or_default(),
                done: vec![false; a.len()],
                actions: a,
                conditions: t["conditions"].as_array().cloned().unwrap_or_default(),
            });
        }
        Ok(b)
    }
    fn add(
        &mut self,
        c: &Config,
        e: &SeedEntity,
        spawned: bool,
        mission: bool,
    ) -> Result<usize, String> {
        let d = c
            .defs
            .get(&e.id)
            .ok_or("Unknown battle entity definition")?;
        let kind = match d.group.as_str() {
            "units" => "unit",
            "decor" => "decor",
            "obstacles" => "obstacle",
            _ => "building",
        };
        let tutorial_friend = self.tutorial.is_some()
            && [2001, 2003, 2005, 2007, 2008, 2009, 2028, 2100].contains(&e.id);
        let own = kind == "unit" && mission && [2100, 2114].contains(&e.id);
        let enemy = kind == "unit"
            && if mission {
                !own && !tutorial_friend
            } else {
                e.fraction != "player"
            };
        let i = self.units.len();
        self.units.push(Unit {
            uid: e.uid.clone(),
            id: e.id,
            kind: kind.into(),
            x: e.x + OFFSET as f64,
            y: e.y + OFFSET as f64,
            direction: e.d.clone(),
            fraction: e.fraction.clone(),
            w: d.width,
            h: d.height,
            hp: d.health,
            hp_max: d.health,
            attack: d.attack,
            distance: d.distance,
            speed: d.speed,
            ability: d.ability,
            heavy: d.heavy,
            enemy,
            own: own || (spawned && kind == "unit" && !enemy),
            mode: if spawned || self.tutorial.is_some() && kind == "unit" {
                2
            } else {
                0
            },
            zone: [0; 4],
            target: None,
            appropriate: e.fraction != "neutral",
            damaged: false,
            profile: None,
            pending: false,
            by_trigger: spawned,
        });
        Ok(i)
    }
    pub fn index(&self, uid: &str) -> Option<usize> {
        self.units.iter().position(|u| u.uid == uid)
    }
    fn alive(&self, i: usize) -> bool {
        self.units[i].hp > 0 && !self.pending_deaths.contains(&i)
    }
    fn own(&self) -> Vec<usize> {
        self.units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.own && u.hp > 0)
            .map(|(i, _)| i)
            .collect()
    }
    fn attackers(&self) -> (Vec<usize>, Vec<usize>) {
        let mut enemies = vec![];
        let mut buildings = vec![];
        for (i, u) in self.units.iter().enumerate() {
            if u.hp < 1 {
                continue;
            }
            if u.kind == "unit" && u.enemy {
                enemies.push(i);
            } else if ["building", "decor"].contains(&u.kind.as_str())
                && u.can_attack()
                && !u.damaged
                && u.appropriate
            {
                buildings.push(i);
            }
        }
        (enemies, buildings)
    }
    pub fn collisions(&self, c: &Config) -> Vec<i32> {
        let m = &c.missions[&self.mission];
        let mut out = vec![0; 16384];
        for e in &self.units {
            if e.pending
                || (e.kind == "unit" && e.hp < 1)
                || (["building", "decor"].contains(&e.kind.as_str()) && e.damaged)
            {
                continue;
            }
            let [mut w, mut h] = e.size();
            let [mut x, mut y] = e.cell();
            if (8000..=8100).contains(&e.id) {
                let fx = e.x - x as f64;
                let fy = e.y - y as f64;
                if fx > 0.25 {
                    x += 1;
                    w = 2;
                }
                if 1. - fx < 0.25 {
                    w = 1;
                }
                if fy > 0.25 {
                    y += 1;
                    h = 2;
                }
                if 1. - fy < 0.25 {
                    h = 1;
                }
            }
            for dx in 0..w {
                for dy in 0..h {
                    let (px, py) = (x - w / 2 + dx, y - h / 2 + dy);
                    if inside(px, py) && out[idx(px, py)] == 0 {
                        out[idx(px, py)] = e.id as i32;
                    }
                }
            }
        }
        let [fx, fy, fw, fh] = m.fog;
        for [x, y, v] in &m.map_data {
            if *v == 7000 && *x >= fx && *x < fx + fw && *y >= fy && *y < fy + fh && inside(*x, *y)
            {
                out[idx(*x, *y)] = 7000;
            }
        }
        for x in 0..GRID {
            for y in 0..GRID {
                if x < fx || x >= fx + fw || y < fy || y >= fy + fh {
                    out[idx(x, y)] = 80000;
                }
            }
        }
        for (row, runs) in m.locked.iter().enumerate() {
            if let Some(a) = runs.as_array() {
                for pair in a.chunks_exact(2) {
                    if let (Some(start), Some(end)) = (pair[0].as_i64(), pair[1].as_i64()) {
                        for y in start..end {
                            let (x, y) = (row as i32 + OFFSET, y as i32 + OFFSET);
                            if inside(x, y) && out[idx(x, y)] == 0 {
                                out[idx(x, y)] = 7008;
                            }
                        }
                    }
                }
            }
        }
        out
    }
    fn free(c: &[i32], p: [i32; 2]) -> bool {
        inside(p[0], p[1]) && c[idx(p[0], p[1])] == 0
    }
    pub fn available(&self, t: u32) -> usize {
        self.roster
            .iter()
            .filter(|u| {
                u.type_id == t
                    && !self
                        .units
                        .iter()
                        .any(|e| e.profile.as_ref() == Some(&u.uid))
            })
            .count()
    }
    /// The preview and drop must agree, including descent zones and tutorial
    /// snapping. A rejected pointer drop is a cancellation, not a model error.
    pub fn placement_cell(&self, c: &Config, p: [i32; 2], t: u32) -> Option<[i32; 2]> {
        if self.pvp_intro_active(c) {
            return None;
        }
        if self.phase != "placement" || self.story.as_ref().is_some_and(|s| s.message.is_some()) {
            return None;
        }
        let p = if self.tutorial.is_some() {
            let step = self.tutorial_step(c)?;
            if ![2, 3].contains(&step.id) || t != 2001 || cheb(p, step.point()) > 1 {
                return None;
            }
            step.point()
        } else {
            p
        };
        let internal = [p[0] + OFFSET, p[1] + OFFSET];
        let m = &c.missions[&self.mission];
        if self.tutorial.is_none() && !m.zones.iter().any(|z| z.kind == 2 && z.contains(internal))
            || !Self::free(&self.collisions(c), internal)
        {
            return None;
        }
        Some(p)
    }
    pub fn place(&mut self, c: &Config, p: [i32; 2], t: u32) -> Result<bool, String> {
        let Some(p) = self.placement_cell(c, p, t) else {
            return Ok(false);
        };
        let Some(profile) = self
            .roster
            .iter()
            .find(|u| {
                u.type_id == t
                    && !self
                        .units
                        .iter()
                        .any(|e| e.profile.as_ref() == Some(&u.uid))
            })
            .map(|u| u.uid.clone())
        else {
            return Ok(false);
        };
        let n = (1..)
            .find(|n| !self.units.iter().any(|u| u.uid == format!("player_{n}")))
            .unwrap();
        let i = self.add(
            c,
            &SeedEntity {
                uid: format!("player_{n}"),
                id: t,
                x: p[0] as f64,
                y: p[1] as f64,
                d: self.landing_direction(c, p).into(),
                fraction: "player".into(),
            },
            false,
            false,
        )?;
        self.units[i].own = true;
        self.units[i].mode = 2;
        self.units[i].profile = Some(profile);
        if self.tutorial.is_some() {
            self.units[i].direction = "se".into();
            self.tutorial_next(c)?;
        }
        Ok(true)
    }
    /// SceneBattle.getStartUnitDirectionOfPosX:Y: faces inward from the
    /// nearest fog edge. Equal distances retain the source NE default.
    pub fn landing_direction(&self, c: &Config, p: [i32; 2]) -> &'static str {
        let [x, y, w, h] = c.missions[&self.mission].fog;
        let (left, top, right, bottom) = (
            p[0] + OFFSET - x,
            p[1] + OFFSET - y,
            x + w - p[0] - OFFSET,
            y + h - p[1] - OFFSET,
        );
        let mut d = "ne";
        if left < top && left < bottom {
            d = "se"
        }
        if top < left && top < right {
            d = "sw"
        }
        if bottom < left && bottom < right {
            d = "ne"
        }
        if right < bottom && right < top {
            d = "nw"
        }
        d
    }
    pub fn reposition(&mut self, c: &Config, uid: &str, p: [i32; 2]) -> bool {
        if self.tutorial.is_some()
            || self.phase != "placement"
            || self.story.as_ref().is_some_and(|s| s.message.is_some())
        {
            return false;
        }
        let Some(i) = self
            .index(uid)
            .filter(|i| self.units[*i].own && self.units[*i].profile.is_some())
        else {
            return false;
        };
        let mut copy = self.clone();
        copy.units.remove(i);
        if copy.placement_cell(c, p, self.units[i].id).is_none() {
            return false;
        }
        let d = self.landing_direction(c, p);
        self.units[i].x = (p[0] + OFFSET) as f64;
        self.units[i].y = (p[1] + OFFSET) as f64;
        self.units[i].direction = d.into();
        true
    }
    pub fn start(&mut self, c: &Config) -> Result<(), String> {
        if self.pvp_intro_active(c) {
            return Err("Сначала закончи инструктаж".into());
        }
        if self.tutorial.is_some() && self.tutorial_id(c) != 4 {
            return Err("Сначала заверши высадку обучения".into());
        }
        if self.phase != "placement"
            || self.story.as_ref().is_some_and(|s| s.message.is_some())
            || !self.units.iter().any(|u| u.profile.is_some())
        {
            return Err("Размести хотя бы один отряд".into());
        }
        self.phase = "battle".into();
        if self.tutorial.is_some() {
            self.tutorial_next(c)?;
        }
        let own = self.own();
        for s in own {
            for i in 0..self.units.len() {
                if self.units[i].enemy
                    && self.units[i].hp > 0
                    && self.units[i].mode == 0
                    && self.units[s].distance_to(&self.units[i]) <= self.units[i].speed
                {
                    self.units[i].mode = 2;
                }
            }
        }
        self.fire(c, 206, json!({}), 0)
    }
    fn melee_pos(&self, c: &Config, t: usize, a: usize) -> Option<[i32; 2]> {
        let v = &self.units[t];
        let u = &self.units[a];
        let [w, h] = v.size();
        let x0 = (v.x - (w / 2) as f64 - 1.) as i32;
        let y0 = (v.y - (h / 2) as f64 - 1.) as i32;
        let x1 = x0 + w + 1;
        let y1 = y0 + h + 1;
        let map = self.collisions(c);
        let mut best = None;
        let mut distance = 100.;
        for x in x0..=x1 {
            for y in y0..=y1 {
                if x != x0 && x != x1 && y != y0 && y != y1 {
                    continue;
                }
                let p = [x, y];
                if p == u.cell() {
                    return Some(p);
                }
                if Self::free(&map, p) {
                    let d = ((u.x - x as f64).powi(2) + (u.y - y as f64).powi(2)).sqrt();
                    if d < distance
                        && trace(&map, u.cell(), p, u.speed, u.id)
                            .is_some_and(|r| r.len() - 1 <= u.speed as usize)
                    {
                        best = Some(p);
                        distance = d;
                    }
                }
            }
        }
        best
    }
    fn queue_attack(&mut self, c: &Config, a: usize, t: usize) -> bool {
        if self.units[a].ability == 10 {
            let Some(pos) = self.melee_pos(c, t, a) else {
                return false;
            };
            let start = self.units[a].cell();
            if pos != start {
                self.queue.push_back(Queued::Move {
                    u: a,
                    end: pos,
                    start: Some(start),
                    route: None,
                });
                self.units[a].x = pos[0] as f64;
                self.units[a].y = pos[1] as f64;
                self.units[a].pending = true;
            }
        }
        self.queue.push_back(Queued::Attack(a, t));
        true
    }
    fn will_die(&self, t: usize) -> bool {
        self.units[t].hp
            - self
                .queue
                .iter()
                .filter_map(|q| {
                    if let Queued::Attack(a, b) = q {
                        (*b == t).then(|| self.units[*a].damage_to(&self.units[t]))
                    } else {
                        None
                    }
                })
                .sum::<i32>()
            < 1
    }
    fn queue_death(&mut self, t: usize, by: usize) {
        if self.pending_deaths.insert(t) {
            self.queue.push_back(Queued::Death(t, by));
        }
    }
    fn run_move(
        &mut self,
        c: &Config,
        u: usize,
        end: [i32; 2],
        start: Option<[i32; 2]>,
    ) -> Option<Vec<[i32; 2]>> {
        let map = self.collisions(c);
        let e = &self.units[u];
        let r = trace(&map, start.unwrap_or(e.cell()), end, e.speed, e.id)?;
        if r.len() - 1 > e.speed as usize || !Self::free(&map, end) {
            return None;
        }
        if r.len() > 1 {
            self.units[u].direction = direction(r[r.len() - 2], end, &self.units[u].direction);
        }
        self.units[u].x = end[0] as f64;
        self.units[u].y = end[1] as f64;
        self.units[u].pending = false;
        Some(r)
    }
    fn execute(&mut self, c: &Config) -> Result<(), String> {
        let mut tutorial_death = false;
        self.visuals.clear();
        let mut events = vec![];
        let mut steps = 0;
        while let Some(q) = self.queue.pop_front() {
            steps += 1;
            if steps > 4096 {
                return Err("Battle queue limit".into());
            }
            match q {
                Queued::Move {
                    u,
                    end,
                    start,
                    route,
                } => {
                    let given = route.is_some();
                    let route = route.or_else(|| self.run_move(c, u, end, start));
                    if route.is_some() && !given && start.is_some() && !self.units[u].enemy {
                        self.energy += 1;
                    }
                    if let Some(r) = route {
                        self.visuals.push(VisualEvent::Move {
                            uid: self.units[u].uid.clone(),
                            route: r.clone(),
                        });
                        let prev = if r.len() > 1 {
                            r[r.len() - 2]
                        } else {
                            [-1, -1]
                        };
                        let e = &self.units[u];
                        events.push((202,json!({"eX0":prev[0],"eY0":prev[1],"eX1":end[0],"eY1":end[1],"type":e.id,"isEnemy":e.enemy,"uid":e.uid,"fraction":e.fraction})));
                        if self.tutorial.is_some() {
                            self.tutorial_moved(c, u)?;
                        }
                    }
                }
                Queued::Attack(a, t) => {
                    if self.units[a].hp > 0 {
                        self.units[a].direction = if self.units[a].kind == "unit" {
                            direction(
                                self.units[a].cell(),
                                self.units[t].cell(),
                                &self.units[a].direction,
                            )
                        } else {
                            let p = self.units[a].cell();
                            let t = self.units[t].cell();
                            let dx = t[0] - p[0];
                            let dy = t[1] - p[1];
                            if dx == 0 && dy == 0 {
                                self.units[a].direction.clone()
                            } else {
                                if dx.abs() < dy.abs() {
                                    if dy < 0 { "ne" } else { "sw" }
                                } else {
                                    if dx < 0 { "nw" } else { "se" }
                                }
                                .into()
                            }
                        };
                        // battleDoAttack:to: starts every queued shooter's
                        // animation before checking the target's health. A
                        // simultaneous lethal shot must not erase crossfire.
                        let was_alive = self.units[t].hp > 0;
                        let damage = if was_alive {
                            self.units[a].damage_to(&self.units[t])
                        } else {
                            0
                        };
                        self.visuals.push(VisualEvent::Attack {
                            uid: self.units[a].uid.clone(),
                            target: self.units[t].uid.clone(),
                            damage,
                            facing: self.units[a].direction.clone(),
                        });
                        self.units[t].hp -= damage;
                        if damage > 0 && self.units[t].kind == "building" {
                            if let Some(p) = &mut self.pvp {
                                let u = &self.units[t];
                                p.hit(
                                    &u.uid,
                                    u.hp,
                                    u.hp_max,
                                    damage,
                                    c.pvp
                                        .as_ref()
                                        .and_then(|c| c.destroy.get(&u.id))
                                        .is_some_and(|a| !a.is_empty()),
                                );
                            }
                        }
                        if damage > 0 {
                            self.log.push(format!(
                                "{} → {}: −{damage}",
                                c.defs[&self.units[a].id].alias, c.defs[&self.units[t].id].alias
                            ));
                        }
                        if was_alive && self.units[t].hp <= 0 {
                            self.queue_death(t, a);
                        }
                    }
                }
                Queued::Death(t, _) => {
                    self.visuals.push(VisualEvent::Death {
                        uid: self.units[t].uid.clone(),
                    });
                    self.pending_deaths.remove(&t);
                    let tutorial_id = self.tutorial_id(c);
                    let e = &mut self.units[t];
                    e.hp = e.hp.min(0);
                    if e.kind != "unit" {
                        e.damaged = true;
                    }
                    e.own = false;
                    if e.enemy && self.tutorial.is_some() && [61, 62].contains(&tutorial_id) {
                        self.tutorial_next(c)?;
                        tutorial_death = true;
                        continue;
                    }
                    events.push((203, json!({"fraction":e.fraction,"uid":e.uid,"type":e.id})));
                }
            }
        }
        for (code, data) in events {
            self.fire(c, code, data, 0)?;
        }
        if tutorial_death {
            Ok(())
        } else {
            self.check_results(c, 0)
        }
    }
    pub fn player_move(&mut self, c: &Config, u: usize, p: [i32; 2]) -> Result<bool, String> {
        if let Some(t) = &self.tutorial {
            if ![106, 107].contains(&self.tutorial_id(c))
                || t.selected.as_deref() != self.units.get(u).map(|u| u.uid.as_str())
                || t.route != Some(p)
            {
                return Ok(false);
            }
        }
        if self.pvp.as_ref().is_some_and(|p| !p.messages.is_empty())
            || self.story.as_ref().is_some_and(|s| s.message.is_some())
            || self.phase != "battle"
            || self.result.is_some()
            || u >= self.units.len()
            || !self.units[u].own
            || !self.alive(u)
        {
            return Ok(false);
        }
        let end = [p[0] + OFFSET, p[1] + OFFSET];
        let Some(r) = self.run_move(c, u, end, None) else {
            return Ok(false);
        };
        if r.len() < 2 {
            return Ok(false);
        }
        self.energy += 1;
        self.queue.push_back(Queued::Move {
            u,
            end,
            start: None,
            route: Some(r),
        });
        if self.tutorial.is_some() {
            self.tutorial.as_mut().unwrap().route = None;
            if !self.enemy_can_attack(c) {
                self.tutorial_enemy_move(c);
            }
        } else {
            self.enemy_turn(c);
        }
        self.execute(c)?;
        Ok(true)
    }
    pub fn player_attack(&mut self, c: &Config, selected: usize, t: usize) -> Result<bool, String> {
        self.player_attack_from(c, Some(selected), t, None)
    }
    pub fn player_attack_all(&mut self, c: &Config, t: usize) -> Result<bool, String> {
        self.player_attack_from(c, None, t, None)
    }
    pub fn player_attack_at(
        &mut self,
        c: &Config,
        selected: usize,
        t: usize,
        p: [i32; 2],
    ) -> Result<bool, String> {
        self.player_attack_from(c, Some(selected), t, Some([p[0] + OFFSET, p[1] + OFFSET]))
    }
    pub fn melee_direction_cell(&self, c: &Config, u: &Unit, t: &Unit, p: [i32; 2]) -> bool {
        if u.ability != 10 || !u.own || u.hp <= 0 || t.hp <= 0 || t.damaged || !t.player_target() {
            return false;
        }
        let [w, h] = t.size();
        let (x, y) = (t.cell()[0] - w / 2 - 1, t.cell()[1] - h / 2 - 1);
        if p[0] < x
            || p[0] > x + w + 1
            || p[1] < y
            || p[1] > y + h + 1
            || p[0] != x && p[0] != x + w + 1 && p[1] != y && p[1] != y + h + 1
        {
            return false;
        }
        if p == u.cell() {
            return true;
        }
        let map = self.collisions(c);
        Self::free(&map, p)
            && trace(&map, u.cell(), p, u.speed, u.id)
                .is_some_and(|r| r.len() - 1 <= u.speed as usize)
    }
    fn player_attack_from(
        &mut self,
        c: &Config,
        selected: Option<usize>,
        t: usize,
        position: Option<[i32; 2]>,
    ) -> Result<bool, String> {
        if let Some(tutorial) = &self.tutorial {
            if selected.is_none()
                || ![61, 62].contains(&self.tutorial_id(c))
                || tutorial.selected.as_deref()
                    != selected
                        .and_then(|i| self.units.get(i))
                        .map(|u| u.uid.as_str())
                || tutorial.enemy.as_deref() != self.units.get(t).map(|u| u.uid.as_str())
            {
                return Ok(false);
            }
        }
        if self.pvp.as_ref().is_some_and(|p| !p.messages.is_empty())
            || self.story.as_ref().is_some_and(|s| s.message.is_some())
            || self.phase != "battle"
            || self.result.is_some()
            || t >= self.units.len()
            || selected
                .is_some_and(|i| i >= self.units.len() || !self.units[i].own || !self.alive(i))
            || !self.alive(t)
        {
            return Ok(false);
        }
        if !self.units[t].player_target() {
            return Ok(false);
        }
        if let Some(p) = position {
            if selected
                .is_none_or(|i| !self.melee_direction_cell(c, &self.units[i], &self.units[t], p))
            {
                return Ok(false);
            }
        }
        let mut fired = false;
        let mut credited = selected;
        for i in 0..self.units.len() {
            let u = &self.units[i];
            let r = if Some(i) == selected {
                u.range()
            } else {
                u.distance
            };
            if u.kind == "unit"
                && u.own
                && self.alive(i)
                && u.distance_to(&self.units[t]) <= r
                && if let Some(pos) = position.filter(|_| Some(i) == selected) {
                    let start = self.units[i].cell();
                    if pos != start {
                        self.queue.push_back(Queued::Move {
                            u: i,
                            end: pos,
                            start: Some(start),
                            route: None,
                        });
                        self.units[i].x = pos[0] as f64;
                        self.units[i].y = pos[1] as f64;
                        self.units[i].pending = true;
                    }
                    self.queue.push_back(Queued::Attack(i, t));
                    true
                } else {
                    self.queue_attack(c, i, t)
                }
            {
                fired = true;
                credited.get_or_insert(i);
            }
        }
        if !fired {
            return Ok(false);
        }
        self.energy += 1;
        if self.will_die(t) {
            self.queue_death(t, credited.unwrap());
        }
        if let Some(tutorial) = self.tutorial.as_mut() {
            tutorial.enemy = None;
        } else {
            self.enemy_turn(c);
        }
        self.execute(c)?;
        Ok(true)
    }
    pub fn skip(&mut self, c: &Config) -> Result<bool, String> {
        if self.tutorial.is_some() {
            return Ok(false);
        }
        if self.pvp.as_ref().is_some_and(|p| !p.messages.is_empty())
            || self.phase != "battle"
            || self.result.is_some()
            || self.story.as_ref().is_some_and(|s| s.message.is_some())
        {
            return Ok(false);
        }
        self.energy += 1;
        self.enemy_turn(c);
        self.execute(c)?;
        Ok(true)
    }
    fn enemy_can_attack(&mut self, c: &Config) -> bool {
        if self.result.is_some() {
            return false;
        }
        let (enemies, buildings) = self.attackers();
        let own = self.own();
        for &i in &enemies {
            if !self.alive(i) {
                self.units[i].target = None;
                continue;
            }
            if let Some(target) = self.units[i].target.clone() {
                for &s in &own {
                    if self.units[s].uid != target {
                        continue;
                    }
                    if self.alive(s) {
                        if self.units[s].distance_to(&self.units[i]) > self.units[i].range()
                            || !self.units[i].can_attack()
                            || (self.units[i].ability == 10 && self.melee_pos(c, s, i).is_none())
                        {
                            self.units[i].target = None;
                            continue;
                        }
                        return true;
                    }
                    self.units[i].target = None;
                }
                if self.units[i].target.is_some() && self.units[i].mode == 2 {
                    return own.iter().any(|s| {
                        self.alive(*s)
                            && self.units[*s].distance_to(&self.units[i]) <= self.units[i].range()
                            && self.units[i].can_attack()
                    });
                }
            }
        }
        for &i in &buildings {
            if !self.alive(i) {
                self.units[i].target = None;
                continue;
            }
            if let Some(target) = self.units[i].target.clone() {
                for &s in &own {
                    if self.units[s].uid != target {
                        continue;
                    }
                    if !self.alive(s) {
                        self.units[i].target = None;
                    }
                    if self.units[i].target.is_some()
                        && self.units[s].distance_to(&self.units[i]) <= self.units[i].distance
                        && self.units[i].can_attack()
                    {
                        return true;
                    }
                    self.units[i].target = None;
                }
            }
        }
        let mut best: Option<usize> = None;
        let mut best_d = -1;
        for &s in &own {
            if !self.alive(s) {
                continue;
            }
            for group in [&enemies, &buildings] {
                for &i in group {
                    if !self.alive(i) {
                        continue;
                    }
                    let e = &self.units[i];
                    let d = self.units[s].distance_to(e);
                    let range = if e.kind == "unit" {
                        e.range()
                    } else {
                        e.distance
                    };
                    if d > range
                        || !e.can_attack()
                        || (e.kind == "unit"
                            && e.ability == 10
                            && self.melee_pos(c, s, i).is_none())
                    {
                        continue;
                    }
                    if best.is_none() {
                        self.units[i].target = Some(self.units[s].uid.clone());
                        best = Some(s);
                        best_d = d;
                    }
                    let b = best.unwrap();
                    if self.units[s].hp < self.units[b].hp
                        || (self.units[s].hp == self.units[b].hp
                            && (d < best_d
                                || (d == best_d
                                    && self.units[b].damage_to(&self.units[i])
                                        < self.units[s].damage_to(&self.units[i]))))
                    {
                        self.units[i].target = Some(self.units[s].uid.clone());
                        best = Some(s);
                        best_d = d;
                    }
                }
            }
        }
        let Some(best) = best else {
            return false;
        };
        let mut top = -1;
        for group in [&enemies, &buildings] {
            let mut holder: Option<usize> = None;
            for &i in group {
                if let Some(target) = &self.units[i].target {
                    let a = self.units[i].damage_to(&self.units[best]);
                    if target != &self.units[best].uid || a <= top {
                        self.units[i].target = None;
                    } else {
                        if let Some(h) = holder {
                            self.units[h].target = None;
                        }
                        holder = Some(i);
                        top = a;
                    }
                }
            }
        }
        true
    }
    fn focus_fire(&mut self, c: &Config, a: usize, t: usize) {
        let (enemies, buildings) = self.attackers();
        for i in enemies.into_iter().chain(buildings) {
            if self.alive(i)
                && i != a
                && self.units[i].can_attack()
                && self.units[t].distance_to(&self.units[i]) <= self.units[i].distance
            {
                self.queue_attack(c, i, t);
            }
        }
        if self.will_die(t) {
            self.queue_death(t, a);
        }
    }
    fn enemy_do_attack(&mut self, c: &Config) {
        let (enemies, buildings) = self.attackers();
        let own = self.own();
        let mut attacked = false;
        for i in enemies {
            let Some(target) = self.units[i].target.clone() else {
                continue;
            };
            if attacked {
                return;
            }
            for &s in &own {
                if self.units[s].uid != target {
                    continue;
                }
                if !self.alive(s) {
                    self.units[i].target = None;
                } else if self.units[i].distance_to(&self.units[s]) <= self.units[i].range() {
                    self.queue_attack(c, i, s);
                    self.focus_fire(c, i, s);
                    return;
                }
                break;
            }
            if self.units[i].mode != 2 {
                break;
            }
            attacked = false;
            for &s in &own {
                if self.alive(s)
                    && self.units[i].distance_to(&self.units[s]) <= self.units[i].range()
                {
                    self.queue_attack(c, i, s);
                    self.focus_fire(c, i, s);
                    attacked = true;
                    break;
                }
            }
        }
        if attacked {
            return;
        }
        for i in buildings {
            if let Some(target) = self.units[i].target.clone() {
                if let Some(s) = own.iter().find(|s| self.units[**s].uid == target) {
                    self.queue_attack(c, i, *s);
                    self.focus_fire(c, i, *s);
                    return;
                }
            }
        }
    }
    fn queue_can_move(&self, u: usize) -> bool {
        !self
            .queue
            .iter()
            .any(|q| matches!(q,Queued::Attack(a,_)|Queued::Death(a,_) if *a==u))
    }
    fn candidates(&self, i: usize, t: [i32; 2], step: i32) -> Vec<[i32; 2]> {
        let e = &self.units[i];
        let [ex, ey] = e.cell();
        let dx = (t[0] as f64 - e.x) as i32;
        let dy = (t[1] as f64 - e.y) as i32;
        let px = (step as f64 * dx.signum() as f64 + e.x) as i32;
        let py = (step as f64 * dy.signum() as f64 + e.y) as i32;
        vec![
            [px, py],
            [px + 1, py],
            [px - 1, py],
            [px, py + 1],
            [px, py - 1],
            [ex, py],
            [px, ey],
        ]
    }
    fn enemy_move_to(&mut self, c: &Config, i: usize, t: [i32; 2]) -> bool {
        let e = &self.units[i];
        let d = cheb(e.cell(), t);
        let map = self.collisions(c);
        if d == 1 {
            if Self::free(&map, t) {
                self.queue.push_back(Queued::Move {
                    u: i,
                    end: t,
                    start: None,
                    route: None,
                });
                return true;
            }
            return false;
        }
        if d < 2 || d >= e.speed * 8 {
            return false;
        }
        for p in self.candidates(i, t, d.min(e.speed)) {
            if Self::free(&map, p)
                && trace(&map, e.cell(), p, 2 * e.speed, e.id)
                    .is_some_and(|r| r.len() - 1 <= e.speed as usize)
            {
                self.queue.push_back(Queued::Move {
                    u: i,
                    end: p,
                    start: None,
                    route: None,
                });
                return true;
            }
        }
        false
    }
    fn route_endpoint(&self, c: &Config, i: usize, t: [i32; 2]) -> [i32; 2] {
        let e = &self.units[i];
        let d = cheb(e.cell(), t);
        let map = self.collisions(c);
        if d <= 1 {
            return if Self::free(&map, t) { t } else { [0, 0] };
        }
        if d >= e.speed * 8 {
            return [0, 0];
        }
        let step = if e.speed >= d { d - 1 } else { e.speed };
        for p in self.candidates(i, t, step) {
            if Self::free(&map, p)
                && trace(&map, e.cell(), p, 2 * e.speed, e.id)
                    .is_some_and(|r| r.len() - 1 <= e.speed as usize)
            {
                return p;
            }
        }
        [0, 0]
    }
    fn move_min_distance(&mut self, c: &Config, i: usize, t: usize) {
        let e = &self.units[i];
        let target = &self.units[t];
        let r = e.range();
        let dx = (target.x - e.x) as i32;
        let dy = (target.y - e.y) as i32;
        let (xmin, xmax, ymin, ymax) = (
            (target.x - r as f64) as i32,
            (target.x + r as f64) as i32,
            (target.y - r as f64) as i32,
            (target.y + r as f64) as i32,
        );
        let mut cells = vec![];
        if dx != 0 {
            let x = if dx < 0 { xmax } else { xmin };
            for y in ymin..=ymax {
                cells.push([x, y]);
            }
        }
        if dy != 0 {
            let y = if dy < 0 { ymax } else { ymin };
            for x in xmin..=xmax {
                cells.push([x, y]);
            }
        }
        let map = self.collisions(c);
        let mut best: Option<Vec<[i32; 2]>> = None;
        for p in cells {
            if !Self::free(&map, p) {
                continue;
            }
            if let Some(path) = trace(&map, e.cell(), p, 30, e.id) {
                if best.as_ref().is_none_or(|b| path.len() < b.len()) {
                    best = Some(path);
                }
            }
        }
        if let Some(p) = best {
            self.route_shortest = p;
        }
        let path = &self.route_shortest;
        if path.is_empty() {
            return;
        }
        let n = (self.units[i].speed as usize).min(path.len() - 1);
        for k in (0..=n).rev() {
            if Self::free(&map, path[k]) {
                self.queue.push_back(Queued::Move {
                    u: i,
                    end: path[k],
                    start: None,
                    route: None,
                });
                return;
            }
        }
        self.queue.push_back(Queued::Move {
            u: i,
            end: path[0],
            start: None,
            route: None,
        });
    }
    fn enemy_do_move(&mut self, c: &Config) {
        let enemies: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.kind == "unit" && u.enemy)
            .map(|(i, _)| i)
            .collect();
        let own = self.own();
        for &i in &enemies {
            let e = &self.units[i];
            if e.hp >= 1 && e.mode == 2 && self.queue_can_move(i) {
                if let Some(target) = &e.target {
                    if let Some(s) = own.iter().find(|s| self.units[**s].uid == *target) {
                        if self.enemy_move_to(c, i, self.units[*s].cell()) {
                            return;
                        }
                    }
                }
            }
        }
        for i in enemies {
            let e = &self.units[i];
            if e.hp < 1 || !self.queue_can_move(i) {
                continue;
            }
            if [1, 2].contains(&e.mode) {
                for &s in &own {
                    if !self.alive(s) {
                        continue;
                    }
                    let mut r = self.units[s].range();
                    if self.units[s].ability == 10 {
                        r /= 2;
                    }
                    if self.units[s].distance_to(&self.units[i]) <= r {
                        self.move_min_distance(c, i, s);
                        return;
                    }
                }
            }
            let e = &self.units[i];
            let center = [e.zone[0] + e.zone[2] / 2, e.zone[1] + e.zone[3] / 2];
            if e.mode == 4 {
                let p = self.route_endpoint(c, i, center);
                if own
                    .iter()
                    .any(|s| self.units[i].distance_to(&self.units[*s]) <= self.units[i].range())
                {
                    for &s in &own {
                        if cheb(p, self.units[s].cell()) <= self.units[i].range() {
                            self.move_min_distance(c, i, s);
                            return;
                        }
                    }
                    return;
                }
            }
            if [3, 4].contains(&self.units[i].mode) && self.enemy_move_to(c, i, center) {
                return;
            }
        }
    }
    fn enemy_turn(&mut self, c: &Config) {
        if self.pvp.is_some() {
            self.pvp_enemy_turn(c);
            return;
        }
        if self.enemy_can_attack(c) {
            self.enemy_do_attack(c);
        } else {
            self.enemy_do_move(c);
        }
    }
    fn event_matches(&self, c: &Config, event: &Value, code: i32, data: &Value) -> bool {
        let d = &event["data"];
        let name = event["name"].as_str().unwrap_or("");
        let ev = match name {
            "UnitEnterOrLeaveZone" => {
                if d["mEv"] == "enter" {
                    200
                } else {
                    201
                }
            }
            "DestroyEntity" => 203,
            "DestroyAllEntity" => 204,
            "BeginPlaceUnits" => 205,
            "EndPlaceUnits" | "StartBattle" => 206,
            _ => -1,
        };
        if ev != code {
            return false;
        }
        let fraction = if name == "DestroyAllEntity" {
            &d["mFraction"]
        } else {
            &d["mUnitFilter"]["mFraction"]
        };
        if let Some(f) = fraction.as_str().filter(|v| *v != "{ANY}") {
            if let Some(actual) = data["fraction"].as_str() {
                if actual != f {
                    return false;
                }
            }
        }
        let f = &d["mUnitFilter"];
        for (key, actual) in [("mItemId", "type"), ("mSceneId", "uid")] {
            let expected = &f[key];
            if !expected.is_null() && expected != "{ANY}" {
                if key == "mItemId" {
                    let n = expected
                        .as_i64()
                        .or_else(|| expected.as_str()?.parse().ok());
                    if data[actual].as_i64() != n {
                        return false;
                    }
                } else if &data[actual] != expected {
                    return false;
                }
            }
        }
        if code == 200 || code == 201 {
            let p0 = [
                data["eX0"].as_i64().unwrap_or(-1) as i32,
                data["eY0"].as_i64().unwrap_or(-1) as i32,
            ];
            let p1 = [
                data["eX1"].as_i64().unwrap_or(-1) as i32,
                data["eY1"].as_i64().unwrap_or(-1) as i32,
            ];
            let z = d["mZoneFilter"]["mZoneId"].as_str();
            return c.missions[&self.mission].zones.iter().any(|zone| {
                (z.is_none() || z == Some("{ANY}") || z == Some(zone.name.as_str()))
                    && if code == 200 {
                        zone.contains(p1) && !zone.contains(p0)
                    } else {
                        zone.contains(p0) && !zone.contains(p1)
                    }
            });
        }
        true
    }
    fn can_spawn(&self, i: usize, fraction: &Value) -> bool {
        self.triggers[i].actions.iter().enumerate().any(|(j, a)| {
            !self.triggers[i].done[j]
                && ["SpawnEntityAction", "ShowEntityAction"]
                    .contains(&a["name"].as_str().unwrap_or(""))
                && a["data"]["mFraction"] == *fraction
        })
    }
    fn fire(&mut self, c: &Config, code: i32, data: Value, depth: usize) -> Result<(), String> {
        if self.story.is_some() {
            return self.story_task(
                c,
                StoryTask::Event {
                    code,
                    data,
                    trigger: 0,
                    event: 0,
                },
            );
        }
        if depth > 256 {
            return Err("Cyclic mission triggers".into());
        }
        let codes = if code == 202 {
            vec![200, 201]
        } else {
            vec![code]
        };
        for i in 0..self.triggers.len() {
            for ev in &codes {
                let t = &self.triggers[i];
                if !t.on {
                    continue;
                }
                if code == 204
                    && (0..self.triggers.len()).any(|o| {
                        o != i && self.triggers[o].on && self.can_spawn(o, &data["fraction"])
                    })
                {
                    continue;
                }
                if t.conditions.is_empty()
                    && t.events
                        .iter()
                        .any(|e| self.event_matches(c, e, *ev, &data))
                {
                    self.perform_next(c, i, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn objective_complete(t: &Trigger) -> bool {
        !t.actions.iter().any(|a| a["name"] == "Complete_Objective") || t.done.iter().all(|v| *v)
    }
    fn perform_next(&mut self, c: &Config, i: usize, depth: usize) -> Result<(), String> {
        if self.story.is_some() {
            return self.story_task(c, StoryTask::Trigger(i));
        }
        self.perform_next_now(c, i, depth)
    }
    fn perform_next_now(&mut self, c: &Config, i: usize, depth: usize) -> Result<(), String> {
        if depth > 256 {
            return Err("Mission action limit".into());
        }
        let t = &self.triggers[i];
        if let Some(j) = (0..t.actions.len()).find(|j| {
            !t.done[*j] && (t.actions[*j]["name"] != "Complete_Objective" || t.actions.len() < 2)
        }) {
            return self.perform(c, i, j, depth + 1);
        }
        self.triggers[i].on = false;
        let pending: Vec<usize> = self.triggers[i]
            .actions
            .iter()
            .enumerate()
            .filter(|(j, a)| a["name"] == "Complete_Objective" && !self.triggers[i].done[*j])
            .map(|(j, _)| j)
            .collect();
        for j in pending {
            self.perform(c, i, j, depth + 1)?;
        }
        Ok(())
    }
    fn perform(&mut self, c: &Config, i: usize, j: usize, depth: usize) -> Result<(), String> {
        if depth > 256 {
            return Err("Mission action limit".into());
        }
        let a = self.triggers[i].actions[j].clone();
        self.triggers[i].done[j] = true;
        let d = &a["data"];
        match a["name"].as_str().unwrap_or("") {
            "MessageBoxAction" => {
                let mask = d
                    .as_str()
                    .or_else(|| d["msg"].as_str())
                    .unwrap_or("")
                    .to_string();
                self.log.push(mask.clone());
                if let Some(story) = self.story.as_mut().filter(|_| self.result.is_none()) {
                    story.message = Some(StoryMessage {
                        canonical_phrases: true,
                        mask,
                        phrase: 0,
                        trigger: i,
                    });
                }
            }
            "MoveCameraAction" => {
                let serial = self
                    .camera_request
                    .as_ref()
                    .map_or(Some(1), |r| r.serial.checked_add(1))
                    .ok_or("Camera request overflow")?;
                let mut point = [0; 2];
                for (i, key) in ["x", "y"].into_iter().enumerate() {
                    point[i] = i32::try_from(
                        d["mPosition"][key]
                            .as_i64()
                            .ok_or("Invalid source camera point")?,
                    )
                    .map_err(|_| "Camera point overflow")?;
                }
                let milliseconds = u32::try_from(
                    d["mDuration"]
                        .as_i64()
                        .ok_or("Invalid source camera duration")?,
                )
                .map_err(|_| "Camera duration overflow")?;
                self.camera_request = Some(crate::battle_camera::Request {
                    serial,
                    point,
                    milliseconds,
                });
            }
            "SetTargetForIA" => {
                let zone = c.missions[&self.mission]
                    .zones
                    .iter()
                    .find(|z| Some(z.name.as_str()) == d["mZoneFilter"]["mZoneId"].as_str())
                    .map_or([0; 4], |z| [z.x + OFFSET, z.y + OFFSET, z.w, z.h]);
                let mode = d["mAIMode"].as_i64().ok_or("Invalid AI mode")? as i32;
                for e in &mut self.units {
                    if Some(e.fraction.as_str()) == d["mFraction"].as_str() {
                        if e.kind == "unit" {
                            e.mode = mode;
                            e.zone = zone;
                        } else if ["building", "decor"].contains(&e.kind.as_str()) {
                            e.mode = mode;
                        }
                    }
                }
            }
            "SpawnEntityAction" | "ShowEntityAction" => {
                let p = [
                    d["mPosition"]["x"].as_i64().ok_or("Invalid spawn x")? as i32 + OFFSET,
                    d["mPosition"]["y"].as_i64().ok_or("Invalid spawn y")? as i32 + OFFSET,
                ];
                let map = self.collisions(c);
                let mut point = p;
                if !Self::free(&map, p) {
                    'search: for r in 1..12 {
                        for dx in -r..=r {
                            for dy in -r..=r {
                                let q = [p[0] + dx, p[1] + dy];
                                if dx.abs().max(dy.abs()) == r && Self::free(&map, q) {
                                    point = q;
                                    break 'search;
                                }
                            }
                        }
                    }
                }
                let e = SeedEntity {
                    uid: format!("spawn_{}", self.units.len()),
                    id: d["mEntityFilter"]["mItemId"]
                        .as_u64()
                        .ok_or("Invalid spawn type")? as u32,
                    x: (point[0] - OFFSET) as f64,
                    y: (point[1] - OFFSET) as f64,
                    d: d["mDir"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("nw")
                        .into(),
                    fraction: d["mFraction"]
                        .as_str()
                        .ok_or("Invalid spawn fraction")?
                        .into(),
                };
                self.add(c, &e, true, false)?;
            }
            "Complete_Objective" => {
                self.log.push(d["lang"].as_str().unwrap_or("").into());
                if self.triggers.iter().all(Self::objective_complete) {
                    self.result = Some("win".into());
                    return Ok(());
                }
            }
            "CloseMissionAction" => {
                let win = d == "complete";
                if !win {
                    for u in &mut self.units {
                        if u.kind == "unit" && !u.enemy && u.hp > 0 {
                            u.hp = 0;
                            u.own = false;
                        }
                    }
                }
                self.result = Some(if win { "win" } else { "lose" }.into());
                return Ok(());
            }
            _ => {}
        }
        self.perform_next(c, i, depth + 1)
    }
    fn check_results(&mut self, c: &Config, depth: usize) -> Result<(), String> {
        if self.pvp.is_some() {
            return self.pvp_results();
        }
        if self.story.is_some() {
            return self.story_task(c, StoryTask::Results(0));
        }
        if self.result.is_some() || self.phase != "battle" {
            return Ok(());
        }
        if self.own().is_empty() {
            self.result = Some("lose".into());
            return Ok(());
        }
        for f in &c.missions[&self.mission].fractions {
            let n = self
                .units
                .iter()
                .filter(|e| {
                    ![3001, 3002, 3010, 3011].contains(&e.id)
                        && ["unit", "building", "decor"].contains(&e.kind.as_str())
                        && e.fraction == *f
                        && e.hp > 0
                })
                .count();
            if n == 0 {
                if f == "player" {
                    self.result = Some("lose".into());
                    return Ok(());
                }
                self.fire(c, 204, json!({"fraction":f}), depth + 1)?;
                if self.result.is_some() {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    /// Reference-compatible view for the cached Python/ARM-derived replay oracle.
    pub fn view(&self) -> Value {
        json!({"energy":self.energy,"phase":self.phase,"result":self.result,
        "ents":self.units.iter().filter(|e|e.kind!="obstacle").map(|e|json!({"uid":e.uid,"id":e.id,"kind":e.kind,"x":e.x-OFFSET as f64,"y":e.y-OFFSET as f64,"hp":e.hp,"mode":e.mode,"enemy":e.enemy,"fraction":e.fraction,"target":e.target,"direction":e.direction})).collect::<Vec<_>>(),
        "objectives":self.triggers.iter().filter(|t|t.actions.iter().any(|a|a["name"]=="Complete_Objective")).map(|t|json!([t.name,Self::objective_complete(t)])).collect::<Vec<_>>()})
    }
}
fn direction(a: [i32; 2], b: [i32; 2], old: &str) -> String {
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

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(p) = &self.pvp {
            p.validate(self)?;
        }
        if let Some(s) = &self.selection {
            s.validate()?;
        }
        if self.defs.len() > 512 || self.missions.len() > 64 {
            return Err("Battle config limit".into());
        }
        if self.hit_clips.len() > 16
            || self.hit_types.len() > 512
            || self
                .hit_types
                .iter()
                .any(|(id, clip)| !self.defs.contains_key(id) || !self.hit_clips.contains_key(clip))
            || self.hit_clips.values().any(|c| {
                c.priority > 100
                    || c.tracks.is_empty()
                    || c.tracks.len() > 16
                    || c.tracks
                        .iter()
                        .any(|t| !t.fps.is_finite() || t.fps <= 0. || t.frames.is_empty())
            })
        {
            return Err("Invalid battle hit catalog".into());
        }
        for d in self.defs.values() {
            if !(1..=16).contains(&d.width)
                || !(1..=16).contains(&d.height)
                || !(0..=1000000).contains(&d.health)
                || !(0..=100000).contains(&d.attack)
                || !(0..=32).contains(&d.speed)
                || !(0..=64).contains(&d.distance)
            {
                return Err("Invalid battle definition".into());
            }
        }
        for (key, m) in &self.missions {
            let [x, y, w, h] = m.fog;
            if key != &m.mission
                || m.entities.len() + m.obstacles.len() > 4096
                || m.triggers.len() > 256
                || w <= 0
                || h <= 0
                || !inside(x, y)
                || !inside(x + w - 1, y + h - 1)
                || m.min_descent > 1000
                || m.map_position.iter().any(|n| n.unsigned_abs() > 10000)
                || !(0..=1000000).contains(&m.unlock_price)
            {
                return Err("Invalid mission geometry".into());
            }
            let mut ids = BTreeSet::new();
            for e in m.entities.iter().chain(&m.obstacles) {
                if !ids.insert(&e.uid)
                    || !self.defs.contains_key(&e.id)
                    || !e.x.is_finite()
                    || !e.y.is_finite()
                    || !(-16.0..112.0).contains(&e.x)
                    || !(-16.0..112.0).contains(&e.y)
                {
                    return Err("Invalid mission seed identity/position".into());
                }
            }
        }
        Ok(())
    }
}
impl Battle {
    pub fn validate(&self, c: &Config) -> Result<(), String> {
        self.validate_with_player(c, &BTreeMap::new())
    }
    /// Mission troops use the current player's recovered Model upgrades;
    /// mission enemies and scenario troops retain their own definitions.
    pub fn validate_with_player(
        &self,
        c: &Config,
        player: &BTreeMap<u32, Definition>,
    ) -> Result<(), String> {
        let m = c
            .missions
            .get(&self.mission)
            .ok_or("Saved battle belongs to unknown mission")?;
        self.validate_tutorial(c)?;
        self.validate_pvp(c)?;
        if let Some(r) = &self.camera_request {
            if r.serial == 0
                || r.serial > 1_000_000
                || r.milliseconds > 60_000
                || r.point.iter().any(|v| !(-16..=128).contains(v))
                || !(self.pvp_camera_valid(c, r)
                    || self.tutorial.as_ref().is_some_and(|t| {
                        c.tutorial_steps.iter().take(t.index).any(|s| {
                            s.action != 0
                                && s.point() == r.point
                                && r.milliseconds == if s.action == 1 { 2000 } else { 1000 }
                        })
                    })
                    || self.triggers.iter().any(|t| {
                        t.actions.iter().zip(&t.done).any(|(a, done)| {
                            *done
                                && a["name"] == "MoveCameraAction"
                                && a["data"]["mPosition"]["x"] == r.point[0]
                                && a["data"]["mPosition"]["y"] == r.point[1]
                                && a["data"]["mDuration"] == r.milliseconds
                        })
                    }))
            {
                return Err("Invalid saved script camera request".into());
            }
        }
        if !["placement", "battle"].contains(&self.phase.as_str())
            || self
                .result
                .as_ref()
                .is_some_and(|s| !["win", "lose"].contains(&s.as_str()))
            || self.energy < 0
            || self.energy > 1000000
            || self.units.len() > 4096
            || self.roster.len() > 1000
            || self.log.len() > 100000
            || !self.queue.is_empty()
            || !self.pending_deaths.is_empty()
        {
            return Err("Invalid saved battle state".into());
        }
        let mut ids = BTreeSet::new();
        let mut profiles = BTreeSet::new();
        let mut roster = BTreeSet::new();
        for r in &self.roster {
            if !roster.insert(&r.uid) || !c.defs.get(&r.type_id).is_some_and(|d| d.group == "units")
            {
                return Err("Invalid mission roster".into());
            }
        }
        for u in &self.units {
            let d = if u.profile.is_some() {
                player.get(&u.id).or_else(|| c.defs.get(&u.id))
            } else {
                c.defs.get(&u.id)
            }
            .ok_or("Missing battle type")?;
            if !ids.insert(&u.uid)
                || !u.x.is_finite()
                || !u.y.is_finite()
                || !inside(u.x as i32, u.y as i32)
                || u.hp > d.health
                || u.hp < -1000000
                || u.hp_max != d.health
                || u.w != d.width
                || u.h != d.height
                || u.attack != d.attack
                || u.speed != d.speed
                || u.distance != d.distance
                || u.ability != d.ability
                || u.heavy != d.heavy
                || u.pending
                || !(0..=4).contains(&u.mode)
                || !["se", "sw", "ne", "nw", "n", "s", "e", "w"].contains(&u.direction.as_str())
            {
                return Err("Invalid saved battle unit".into());
            }
            if let Some(p) = &u.profile {
                if !profiles.insert(p)
                    || !self.roster.iter().any(|r| r.uid == *p && r.type_id == u.id)
                    || u.enemy
                    || u.kind != "unit"
                {
                    return Err("Invalid mission unit ownership".into());
                }
            }
            if let Some(t) = &u.target {
                if !self.units.iter().any(|e| &e.uid == t) {
                    return Err("Invalid AI target".into());
                }
            }
        }
        if m.entities
            .iter()
            .chain(&m.obstacles)
            .any(|s| !self.units.iter().any(|u| u.uid == s.uid && u.id == s.id))
        {
            return Err("Mission seed object missing".into());
        }
        let initial = Self::new(c, &self.mission, self.roster.clone())?;
        if let Some(s) = &self.story {
            if s.pumping
                || s.tasks.len() > 1024
                || s.message.as_ref().is_some_and(|m| {
                    m.phrase > 1000
                        || !self.triggers.get(m.trigger).is_some_and(|t| {
                            t.actions.iter().zip(&t.done).any(|(a, done)| {
                                *done
                                    && a["name"] == "MessageBoxAction"
                                    && (a["data"].as_str().or_else(|| a["data"]["msg"].as_str())
                                        == Some(m.mask.as_str()))
                            })
                        })
                })
                || s.tasks.iter().any(|t| match t {
                    StoryTask::Trigger(i) => *i >= self.triggers.len(),
                    StoryTask::Event {
                        code,
                        trigger,
                        event,
                        ..
                    } => {
                        !(200..=206).contains(code)
                            || *trigger > self.triggers.len()
                            || *event > usize::from(*code == 202)
                    }
                    StoryTask::Results(f) => *f > m.fractions.len(),
                })
            {
                return Err("Invalid saved story continuation".into());
            }
        }
        if self.triggers.len() != initial.triggers.len() {
            return Err("Saved mission triggers changed".into());
        }
        for (t, s) in self.triggers.iter().zip(&initial.triggers) {
            if t.name != s.name
                || t.events != s.events
                || t.actions != s.actions
                || t.conditions != s.conditions
                || t.done.len() != s.done.len()
            {
                return Err("Saved mission script differs from source".into());
            }
        }
        Ok(())
    }
    pub fn unplace(&mut self, uid: &str) -> bool {
        if self.tutorial.is_some() {
            return false;
        }
        if self.phase != "placement" || self.story.as_ref().is_some_and(|s| s.message.is_some()) {
            return false;
        }
        let Some(i) = self.index(uid).filter(|i| self.units[*i].profile.is_some()) else {
            return false;
        };
        self.units.remove(i);
        true
    }

    pub fn enable_story(&mut self, c: &Config) -> Result<(), String> {
        if self.pvp.is_some() {
            return Ok(());
        }
        if self.tutorial.is_some() {
            return Ok(());
        }
        if self.story.is_some() {
            return Err("Mission story is already initialized".into());
        }
        self.story = Some(StoryFlow {
            message: None,
            tasks: VecDeque::new(),
            pumping: false,
        });
        // BeginPlaceUnits is source event205; it precedes user deployment.
        self.fire(c, 205, json!({}), 0)
    }
    pub fn advance_story(&mut self, c: &Config, phrase_count: usize) -> Result<(), String> {
        let story = self.story.as_mut().ok_or("Нет разговора")?;
        let message = story.message.as_mut().ok_or("Нет реплики")?;
        if phrase_count == 0 || message.phrase >= phrase_count {
            return Err("Invalid story phrase".into());
        }
        message.phrase += 1;
        if message.phrase == phrase_count {
            story.message = None;
            self.pump_story(c)?;
        }
        Ok(())
    }
    fn story_task(&mut self, c: &Config, task: StoryTask) -> Result<(), String> {
        let s = self.story.as_mut().ok_or("Missing story continuation")?;
        if s.pumping {
            s.tasks.push_front(task);
        } else {
            s.tasks.push_back(task);
        }
        self.pump_story(c)
    }
    fn pump_story(&mut self, c: &Config) -> Result<(), String> {
        if self
            .story
            .as_ref()
            .is_none_or(|s| s.pumping || s.message.is_some())
        {
            return Ok(());
        }
        self.story.as_mut().unwrap().pumping = true;
        let result = (|| {
            for _ in 0..4096 {
                if self.story.as_ref().unwrap().message.is_some() {
                    return Ok(());
                }
                let Some(task) = self.story.as_mut().unwrap().tasks.pop_front() else {
                    return Ok(());
                };
                match task {
                    StoryTask::Trigger(i) => self.perform_next_now(c, i, 0)?,
                    StoryTask::Event {
                        code,
                        data,
                        trigger,
                        event,
                    } => {
                        let codes: &[i32] = if code == 202 {
                            &[200, 201]
                        } else {
                            std::slice::from_ref(&code)
                        };
                        if trigger >= self.triggers.len() {
                            continue;
                        }
                        let (next_trigger, next_event) = if event + 1 < codes.len() {
                            (trigger, event + 1)
                        } else {
                            (trigger + 1, 0)
                        };
                        self.story
                            .as_mut()
                            .unwrap()
                            .tasks
                            .push_front(StoryTask::Event {
                                code,
                                data: data.clone(),
                                trigger: next_trigger,
                                event: next_event,
                            });
                        let t = &self.triggers[trigger];
                        if t.on
                            && t.conditions.is_empty()
                            && !(code == 204
                                && (0..self.triggers.len()).any(|o| {
                                    o != trigger
                                        && self.triggers[o].on
                                        && self.can_spawn(o, &data["fraction"])
                                }))
                            && t.events
                                .iter()
                                .any(|e| self.event_matches(c, e, codes[event], &data))
                        {
                            self.perform_next(c, trigger, 0)?;
                        }
                    }
                    StoryTask::Results(fraction) => {
                        if self.result.is_some() || self.phase != "battle" {
                            continue;
                        }
                        if self.own().is_empty() {
                            self.result = Some("lose".into());
                            continue;
                        }
                        let Some(f) = c.missions[&self.mission].fractions.get(fraction) else {
                            continue;
                        };
                        self.story
                            .as_mut()
                            .unwrap()
                            .tasks
                            .push_front(StoryTask::Results(fraction + 1));
                        let empty = !self.units.iter().any(|e| {
                            ![3001, 3002, 3010, 3011].contains(&e.id)
                                && ["unit", "building", "decor"].contains(&e.kind.as_str())
                                && e.fraction == *f
                                && e.hp > 0
                        });
                        if empty {
                            if f == "player" {
                                self.result = Some("lose".into());
                            } else {
                                self.fire(c, 204, json!({"fraction":f}), 0)?;
                            }
                        }
                    }
                }
            }
            Err("Mission story continuation limit".into())
        })();
        self.story.as_mut().unwrap().pumping = false;
        result
    }
}

impl Battle {
    pub fn scene(&self, base: &crate::Scene) -> Result<crate::Scene, String> {
        self.scene_with_models(base, &BTreeMap::new())
    }
    pub fn scene_with_models(
        &self,
        base: &crate::Scene,
        models: &BTreeMap<u32, String>,
    ) -> Result<crate::Scene, String> {
        // Keep the full public Scene API for inspection/model clients. The
        // interactive controller explicitly requests the lean render view.
        let world = self.render_scene_with_models(base, models)?;
        let mut scene = base.clone();
        scene.interface = None;
        scene.world = None;
        scene.terrain = world.terrain;
        scene.ground_details = world.ground_details;
        scene.ground_detail_layer = world.ground_detail_layer;
        scene.camera = world.camera;
        scene.entities = world.entities;
        Ok(scene)
    }
    pub fn render_scene_with_models(
        &self,
        base: &crate::Scene,
        models: &BTreeMap<u32, String>,
    ) -> Result<crate::Scene, String> {
        let c = base
            .combat
            .as_ref()
            .ok_or("Missing mission renderer config")?;
        let v = c
            .views
            .get(&self.mission)
            .ok_or("Missing original mission artwork")?;
        // A render view owns only world geometry and entity tracks. Cloning
        // every mission, animation and UI catalog on each shot caused a pause;
        // the renderer borrows those immutable assets from the source Scene.
        let mut scene = crate::Scene {
            achievements: None,
            obstacles: None,
            quest_navigation: None,
            expansions: None,
            localization: None,
            upgrades: None,
            shop: None,
            interface: None,
            audio: None,
            format: base.format.clone(),
            projection: base.projection.clone(),
            camera: v.camera,
            textures: base.textures.clone(),
            terrain: v.terrain.clone(),
            ground_details: v.ground_details.clone(),
            ground_detail_layer: v.ground_detail_layer,
            entities: vec![],
            resources: base.resources.clone(),
            economy: None,
            progression: None,
            world: None,
            construction: None,
            lifecycle: None,
            workshop: None,
            research: None,
            pricing: None,
            hud: base.hud.clone(),
            combat: None,
            presentation: base.presentation.clone(),
            font: base.font.clone(),
            provenance: base.provenance.clone(),
        };
        // Base fog/hit areas belong to SceneMain. Keeping them clips mission
        // troops and clamps the battle camera to an unrelated base rectangle.
        scene.world = None;
        for u in &self.units {
            // createObjectBuilding0x4f38d omits damaged fences3000..3100;
            // ordinary destroyed buildings retain their original ruin artwork.
            if (u.kind == "unit" && u.hp <= 0) || (u.damaged && (3000..=3100).contains(&u.id)) {
                continue;
            }
            let x = u.x - OFFSET as f64;
            let y = u.y - OFFSET as f64;
            let model = if u.profile.is_some() && !u.enemy {
                models.get(&u.id)
            } else {
                None
            };
            let black_key = format!(
                "{}/black/{}/{}",
                u.id,
                u.direction,
                if u.damaged { "damaged" } else { "done" }
            );
            let key = if u.kind == "building"
                && self.tutorial.is_none()
                && u.fraction != "player"
                && c.sprites.contains_key(&black_key)
            {
                black_key
            } else if u.kind == "unit" {
                format!(
                    "{}/{}/{}",
                    model.cloned().unwrap_or_else(|| u.id.to_string()),
                    if u.enemy { "black" } else { "yellow" },
                    u.direction
                )
            } else if u.kind == "obstacle" {
                format!("{}/static", u.id)
            } else {
                format!(
                    "{}/{}/{}",
                    u.id,
                    u.direction,
                    if u.damaged { "damaged" } else { "done" }
                )
            };
            let tracks = if u.kind == "decor" {
                v.fences
                    .get(&format!("{},{}", x as i32, y as i32))
                    .or_else(|| v.sprites.get(&key))
            } else {
                v.sprites.get(&key)
            };
            let tracks = tracks
                .filter(|t| !t.is_empty())
                .or_else(|| c.sprites.get(&key).filter(|t| !t.is_empty()))
                .ok_or_else(|| format!("Missing original battle artwork: {key}"))?
                .clone();
            let d = &c.defs[&u.id];
            let direction = match u.direction.as_str() {
                "se" => 0x100000,
                "sw" => 0x200000,
                "ne" => 0x400000,
                "nw" => 0x800000,
                "n" => 0x1000000,
                "s" => 0x2000000,
                "e" => 0x4000000,
                "w" => 0x8000000,
                _ => return Err("Unknown battle facing".into()),
            };
            scene.entities.push(crate::Entity{uid:u.uid.clone(),type_id:u.id,kind:u.kind.clone(),name:d.alias.clone(),x,y,direction,width:u.w as f64,height:u.h as f64,
                archived:json!({"uid":u.uid,"type":u.id,"x":u.x,"y":u.y,"direction":direction,"state":if u.damaged{0}else{3},"hp":u.hp,"hp_max":u.hp_max,"enemy":u.enemy,"visual_model":model}),tracks});
        }
        scene.entities.sort_by(|a, b| {
            (a.y * 128. + a.x)
                .partial_cmp(&(b.y * 128. + b.x))
                .unwrap()
                .then(a.uid.cmp(&b.uid))
        });
        Ok(scene)
    }
}

impl Config {
    pub fn validate_art(&self, scene: &crate::Scene) -> Result<(), String> {
        let region = |texture: &str, src: [u32; 4], rect: [f64; 4]| -> Result<(), String> {
            let t = scene.textures.get(texture).ok_or("Missing mission PNG")?;
            if src[2] == 0
                || src[3] == 0
                || src[0].checked_add(src[2]).is_none_or(|x| x > t.width)
                || src[1].checked_add(src[3]).is_none_or(|y| y > t.height)
                || !rect.iter().all(|x| x.is_finite())
                || rect[2] <= 0.
                || rect[3] <= 0.
            {
                return Err("Mission sprite outside original atlas".into());
            }
            Ok(())
        };
        for (name, v) in &self.views {
            if !self.missions.contains_key(name)
                || !v.camera.iter().all(|x| x.is_finite())
                || !(0.12..=2.).contains(&v.camera[2])
                || v.terrain.len() > 100000
                || v.ground_details.len() > 20000
                || v.ground_detail_layer.is_some_and(|i| i > v.terrain.len())
                || v.sprites.len() > 4096
            {
                return Err("Invalid mission visual scope".into());
            }
            for t in v.terrain.iter().chain(&v.ground_details) {
                region(&t.texture, t.src, t.rect)?;
            }
        }
        if let Some(s) = &self.selection {
            for t in s
                .tiles
                .values()
                .chain(s.cursor.values())
                .chain(s.directions.values())
            {
                for f in t.frames.iter() {
                    for p in f {
                        region(&t.texture, p.src, p.rect)?;
                    }
                }
            }
        }
        for tracks in self
            .sprites
            .values()
            .chain(self.animations.values())
            .chain(self.hit_clips.values().map(|c| &c.tracks))
            .chain(
                self.views
                    .values()
                    .flat_map(|v| v.sprites.values().chain(v.fences.values())),
            )
        {
            for t in tracks {
                if !t.fps.is_finite()
                    || !(0.0..=60.).contains(&t.fps)
                    || t.frames.is_empty()
                    || t.frames.len() > 256
                {
                    return Err("Invalid mission animation".into());
                }
                for f in t.frames.iter() {
                    for p in f {
                        region(&t.texture, p.src, p.rect)?;
                    }
                }
            }
        }
        Ok(())
    }
}
