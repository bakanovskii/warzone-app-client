//! SceneBattle selection/route/target presentation. Entirely transient: selecting
//! or previewing does not spend energy, reserve troops or write a game receipt.
use crate::{
    Camera, Scene, Track,
    battle::{self, Battle, OFFSET, Unit},
    interface::{Draw, View},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub tiles: BTreeMap<String, Track>,
    pub cursor: BTreeMap<String, Track>,
    pub directions: BTreeMap<String, Track>,
    #[serde(default)]
    pub highlights: BTreeMap<String, Track>,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.tiles.len() != 3
            || self.cursor.len() != 4
            || self.directions.len() != 8
            || ["move", "stop", "unit"]
                .iter()
                .any(|k| !self.tiles.contains_key(*k))
            || ["ne", "sw", "se", "nw"]
                .iter()
                .any(|k| !self.cursor.contains_key(*k))
            || (!self.highlights.is_empty() && self.highlights.len() != 7)
            || (!self.highlights.is_empty()
                && ["1x1", "2x2", "2x3_right", "2x3_left", "3x3", "4x4", "5x5"]
                    .iter()
                    .any(|k| !self.highlights.contains_key(*k)))
            || self
                .tiles
                .values()
                .chain(self.cursor.values())
                .chain(self.directions.values())
                .chain(self.highlights.values())
                .any(|t| {
                    !t.fps.is_finite() || t.fps < 0. || t.frames.is_empty() || t.frames.len() > 512
                })
        {
            return Err("Invalid original battle selection assets".into());
        }
        Ok(())
    }
}
type MovementFieldCache = Option<(String, [i32; 2], i32, Vec<i32>)>;
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub unit: Option<String>,
    pub target: Option<String>,
    pub route: Vec<[i32; 2]>,
    field: std::cell::RefCell<MovementFieldCache>,
    highlighted: std::cell::RefCell<Option<Vec<String>>>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    None,
    Move(String, [i32; 2]),
    Attack(String, String),
    AttackAll(String),
    AttackFrom(String, String, [i32; 2]),
}
fn live(u: &Unit) -> bool {
    u.hp > 0 && !u.damaged
}
fn dimensions(u: &Unit) -> (i32, i32) {
    if matches!(u.direction.as_str(), "sw" | "ne") {
        (u.h, u.w)
    } else {
        (u.w, u.h)
    }
}
pub fn route(b: &Battle, c: &battle::Config, uid: &str, cell: [i32; 2]) -> Option<Vec<[i32; 2]>> {
    let u = b.units.get(b.index(uid)?)?;
    if !u.own || !live(u) || u.kind != "unit" {
        return None;
    }
    let end = [cell[0] + OFFSET, cell[1] + OFFSET];
    if !end.iter().all(|v| (1..=126).contains(v)) {
        return None;
    }
    let map = b.collisions(c);
    if map[(end[0] * 128 + end[1]) as usize] != 0 {
        return None;
    }
    let p = battle::trace(&map, u.cell(), end, u.speed, u.id)?;
    (p.len() > 1 && p.len() - 1 <= u.speed as usize).then_some(p)
}
impl Selection {
    pub fn clear_pending(&mut self) {
        self.target = None;
        self.route.clear();
    }
    pub fn reconcile(&mut self, b: &Battle) {
        self.field.get_mut().take();
        self.highlighted.get_mut().take();
        self.clear_pending();
        if self.unit.as_ref().is_some_and(|uid| {
            b.index(uid)
                .is_none_or(|i| !live(&b.units[i]) || !b.units[i].own)
        }) {
            self.unit = None;
        }
        if self
            .target
            .as_ref()
            .is_some_and(|uid| b.index(uid).is_none_or(|i| !live(&b.units[i])))
        {
            self.target = None;
        }
        self.route.clear();
    }
    /// SceneBattle.onTouch: first tap shows, second confirms the same target/cell.
    pub fn click(
        &mut self,
        b: &Battle,
        c: &battle::Config,
        picked: Option<&str>,
        cell: [i32; 2],
    ) -> Intent {
        if b.phase != "battle" || b.result.is_some() {
            return Intent::None;
        }
        if let (Some(uid), Some(tid)) = (&self.unit, &self.target) {
            if let (Some(u), Some(t)) = (
                b.index(uid).map(|i| &b.units[i]),
                b.index(tid).map(|i| &b.units[i]),
            ) {
                if b.melee_direction_cell(c, u, t, [cell[0] + OFFSET, cell[1] + OFFSET]) {
                    return Intent::AttackFrom(uid.clone(), tid.clone(), cell);
                }
            }
        }
        if let Some(t) = picked
            .and_then(|uid| b.index(uid))
            .map(|i| &b.units[i])
            .filter(|u| live(u))
        {
            if t.own && t.kind == "unit" {
                self.unit = Some(t.uid.clone());
                self.clear_pending();
                return Intent::None;
            }
            if t.player_target() {
                self.route.clear();
                if self.target.as_ref() == Some(&t.uid) {
                    if let Some(uid) = &self.unit {
                        return Intent::Attack(uid.clone(), t.uid.clone());
                    }
                    return Intent::AttackAll(t.uid.clone());
                } else {
                    self.target = Some(t.uid.clone());
                }
            }
            return Intent::None;
        }
        self.target = None;
        let Some(uid) = &self.unit else {
            return Intent::None;
        };
        if self.route.last() == Some(&[cell[0] + OFFSET, cell[1] + OFFSET]) {
            return Intent::Move(uid.clone(), cell);
        }
        self.route = route(b, c, uid, cell).unwrap_or_default();
        if self.route.is_empty() {
            if let Some(u) = b.index(uid).map(|i| &b.units[i]) {
                if (cell[0] + OFFSET - u.cell()[0])
                    .abs()
                    .max((cell[1] + OFFSET - u.cell()[1]).abs())
                    > u.speed
                {
                    self.unit = None;
                }
            }
        }
        Intent::None
    }
}
/// Original groups by unit type in an NSDictionary. Its enumeration is not a
/// gameplay rule; keep the existing deterministic native order for now.
pub fn types(b: &Battle) -> Vec<u32> {
    let mut ids: Vec<_> = b.roster.iter().map(|u| u.type_id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}
pub fn sprite(
    v: &mut View,
    scene: &Scene,
    camera: &Camera,
    track: &Track,
    position: [f64; 2],
    seconds: f64,
    opacity: u8,
    under: bool,
) -> Option<()> {
    let template = scene
        .interface
        .as_ref()?
        .symbols
        .get("unit_health")?
        .nodes
        .first()?;
    for part in track.frame(seconds) {
        let mut n = template.clone();
        n.id = "source_battle_marker".into();
        n.parents = vec![
            if under {
                "battle_underlay"
            } else {
                "battle_overlay"
            }
            .into(),
        ];
        n.texture = track.texture.clone();
        n.source = Some(part.src);
        let p = camera.screen([position[0] + part.rect[0], position[1] + part.rect[1]]);
        n.rect = [
            p[0],
            p[1],
            part.rect[2] * camera.zoom,
            part.rect[3] * camera.zoom,
        ];
        v.draws.push(Draw {
            node: n,
            clip: None,
            progress: 1.,
            opacity,
            grayscale: false,
        });
    }
    Some(())
}
pub fn markers(
    v: &mut View,
    scene: &Scene,
    b: &Battle,
    camera: &Camera,
    selection: &Selection,
    seconds: f64,
) -> Option<()> {
    let c = scene.combat.as_ref()?;
    let art = c.selection.as_ref()?;
    v.retained_battle_selection = true;
    if b.phase == "placement" && b.result.is_none() {
        if let Some(u) = selection
            .target
            .as_ref()
            .or(selection.unit.as_ref())
            .and_then(|id| b.index(id))
            .map(|i| &b.units[i])
            .filter(|u| live(u))
        {
            let r = u.range() as f64;
            if r > 0. {
                let (x, y) = (u.x - OFFSET as f64 - r, u.y - OFFSET as f64 - r);
                let n = 2. * r + 1.;
                v.battle_attack_area = [[x, y], [x + n, y], [x + n, y + n], [x, y + n], [x, y]]
                    .map(|p| camera.screen(scene.projection.project(p[0], p[1])))
                    .to_vec();
            }
            sprite(
                v,
                scene,
                camera,
                &art.tiles["unit"],
                scene
                    .projection
                    .project(u.x - OFFSET as f64, u.y - OFFSET as f64),
                seconds,
                255,
                true,
            );
        }
        return Some(());
    }
    if b.phase != "battle" || b.result.is_some() {
        return Some(());
    }
    let selected = selection
        .unit
        .as_ref()
        .and_then(|s| b.index(s))
        .map(|i| &b.units[i])
        .filter(|u| live(u));
    if !art.highlights.is_empty() {
        let mut cache = selection.highlighted.borrow_mut();
        if cache.is_none() {
            *cache = Some(highlighted_targets(b, c));
        }
        for uid in cache.as_ref()? {
            let t = &b.units[b.index(uid)?];
            let (w, h) = if t.kind == "unit" {
                (1, 1)
            } else {
                dimensions(t)
            };
            let key = match (w, h) {
                (1, 1) => "1x1",
                (2, 2) => "2x2",
                (2, 3) => "2x3_right",
                (3, 2) => "2x3_left",
                (3, 3) => "3x3",
                (4, 4) => "4x4",
                (5, 5) => "5x5",
                _ => continue,
            };
            let factor = if matches!(t.id, 1003 | 1006 | 1019 | 1020 | 1034 | 1038 | 1501 | 1502) {
                if (w, h) == (2, 3) { 2.25 } else { 2. }
            } else {
                1.8
            };
            let p = scene
                .projection
                .project(t.x - OFFSET as f64, t.y - OFFSET as f64);
            let anchor = camera.screen(p);
            let begin = v.draws.len();
            sprite(
                v,
                scene,
                camera,
                &art.highlights[key],
                p,
                seconds,
                255,
                true,
            )?;
            for d in &mut v.draws[begin..] {
                d.node.id = format!("source_enemy_frame:{}", t.uid);
                for (axis, anchor) in anchor.iter().enumerate() {
                    d.node.rect[axis] = anchor + (d.node.rect[axis] - anchor) * factor;
                    d.node.rect[axis + 2] *= factor;
                }
            }
        }
    }
    if let Some(u) = selected {
        let start = u.cell();
        let mut cached = selection.field.borrow_mut();
        if cached
            .as_ref()
            .is_none_or(|(uid, p, s, _)| uid != &u.uid || *p != start || *s != u.speed)
        {
            *cached = Some((
                u.uid.clone(),
                start,
                u.speed,
                battle::distance_map(&b.collisions(c), start, u.speed, u.id)?,
            ));
        }
        let distances = &cached.as_ref()?.3;
        let map = b.collisions(c);
        for dx in -u.speed..=u.speed {
            for dy in -u.speed..=u.speed {
                let (x, y) = (start[0] + dx, start[1] + dy);
                if !(0..128).contains(&x) || !(0..128).contains(&y) {
                    continue;
                }
                let k = (x * 128 + y) as usize;
                let occupied = map[k] != 0;
                let center = dx == 0 && dy == 0;
                let enemy_unit = b
                    .units
                    .iter()
                    .any(|e| e.kind == "unit" && e.enemy && live(e) && e.cell() == [x, y]);
                let (key, alpha) = if occupied && !center {
                    if enemy_unit {
                        continue;
                    }
                    ("stop", 255)
                } else if center || distances[k] > 0 && distances[k] - 1 <= u.speed {
                    ("move", 128)
                } else {
                    continue;
                };
                sprite(
                    v,
                    scene,
                    camera,
                    &art.tiles[key],
                    scene
                        .projection
                        .project((x - OFFSET) as f64, (y - OFFSET) as f64),
                    seconds,
                    alpha,
                    true,
                );
            }
        }
        let p = scene
            .projection
            .project(u.x - OFFSET as f64, u.y - OFFSET as f64);
        sprite(v, scene, camera, &art.tiles["unit"], p, seconds, 255, true);
        v.battle_route = selection
            .route
            .iter()
            .map(|p| {
                let p = scene
                    .projection
                    .project((p[0] - OFFSET) as f64, (p[1] - OFFSET) as f64);
                camera.screen([p[0], p[1] + 32.])
            })
            .collect();
        if selection.target.is_none() && u.range() > 0 {
            let r = u.range() as f64;
            let (x, y) = (u.x - OFFSET as f64 - r, u.y - OFFSET as f64 - r);
            let n = r * 2. + 1.;
            v.battle_attack_area = [[x, y], [x + n, y], [x + n, y + n], [x, y + n], [x, y]]
                .map(|p| camera.screen(scene.projection.project(p[0], p[1])))
                .to_vec();
        }
    }
    // Attack cursors belong to the explicitly selected target. General red
    // reachability frames are a separate original layer, drawn above.
    for t in b
        .units
        .iter()
        .filter(|t| live(t) && t.player_target() && selection.target.as_ref() == Some(&t.uid))
    {
        let (w, h) = if matches!(t.direction.as_str(), "sw" | "ne") {
            (t.h, t.w)
        } else {
            (t.w, t.h)
        };
        let (a0, a1, b0, b1) = (-(w / 2), w - w / 2, -(h / 2), h - h / 2);
        let p = |a, b| [62. * (a - b) as f64, 32. * (a + b) as f64];
        let mid = |a: [f64; 2], b: [f64; 2], dx: f64| {
            [
                ((a[0] + b[0]) / 2.).trunc() + dx,
                ((a[1] + b[1]) / 2.).trunc(),
            ]
        };
        let base = scene
            .projection
            .project(t.x - OFFSET as f64, t.y - OFFSET as f64);
        let r = t.range() as f64;
        if r > 0. && selection.target.as_ref() == Some(&t.uid) {
            let x = t.x - OFFSET as f64 - r;
            let y = t.y - OFFSET as f64 - r;
            let n = r * 2. + 1.;
            v.battle_attack_area = [[x, y], [x + n, y], [x + n, y + n], [x, y + n], [x, y]]
                .map(|p| camera.screen(scene.projection.project(p[0], p[1])))
                .to_vec();
        }
        let can_attack = b.units.iter().any(|u| {
            u.kind == "unit"
                && u.own
                && live(u)
                && u.distance_to(t)
                    <= if selected.is_some_and(|s| u.uid == s.uid) {
                        u.range()
                    } else {
                        u.distance
                    }
        });
        if selection.target.as_ref() == Some(&t.uid) {
            if let Some(u) = selected.filter(|u| u.ability == 10) {
                // showAttackDirectionAreaForUnit:AndVictim:0x6a719 uses all
                // perimeter cells, not just eight guessed positions around a building.
                let x0 = t.cell()[0] - w / 2 - 1;
                let y0 = t.cell()[1] - h / 2 - 1;
                let x1 = x0 + w + 1;
                let y1 = y0 + h + 1;
                let mut tiles = vec![
                    ("NW", [x0, y0]),
                    ("NE", [x1, y0]),
                    ("SW", [x0, y1]),
                    ("SE", [x1, y1]),
                ];
                for x in x0 + 1..x1 {
                    tiles.push(("N", [x, y0]));
                    tiles.push(("S", [x, y1]));
                }
                for y in y0 + 1..y1 {
                    tiles.push(("W", [x0, y]));
                    tiles.push(("E", [x1, y]));
                }
                let map = b.collisions(c);
                let cached = selection.field.borrow();
                let distances = &cached.as_ref()?.3;
                for (direction, cell) in tiles {
                    // trace uses this same distance field. Reuse the selected squad's
                    // cached field instead of rebuilding the128x128 wave for every arrow.
                    if cell.iter().all(|v| (1..127).contains(v)) && {
                        let k = (cell[0] * 128 + cell[1]) as usize;
                        map[k] == 0 && distances[k] > 0 && distances[k] - 1 <= u.speed
                    } {
                        sprite(
                            v,
                            scene,
                            camera,
                            &art.directions[direction],
                            scene
                                .projection
                                .project((cell[0] - OFFSET) as f64, (cell[1] - OFFSET) as f64),
                            seconds,
                            255,
                            false,
                        );
                    }
                }
            }
        }
        if can_attack && !selected.is_some_and(|u| u.ability == 10) {
            for (d, at) in [
                ("ne", mid(p(a0, b0), p(a1, b0), -33.)),
                ("sw", mid(p(a1, b1), p(a0, b1), 0.)),
                ("se", mid(p(a1, b0), p(a1, b1), -33.)),
                ("nw", mid(p(a0, b1), p(a0, b0), 0.)),
            ] {
                sprite(
                    v,
                    scene,
                    camera,
                    &art.cursor[d],
                    [base[0] + at[0], base[1] + at[1]],
                    seconds,
                    255,
                    false,
                );
            }
        }
    }
    // The movement area must not wash a reachable enemy's red outline green.
    // These remain world underlays (below the actors), above the walk tiles.
    let mut frames = Vec::new();
    v.draws.retain(|d| {
        if d.node.id.starts_with("source_enemy_frame:") {
            frames.push(d.clone());
            false
        } else {
            true
        }
    });
    v.draws.extend(frames);
    Some(())
}

// SceneBattle.isEnemyAvailableForFire0x58cbd checks all own squads, even before
// selection. Melee uses the reachable area, rather than distance through walls.
fn highlighted_targets(b: &Battle, c: &battle::Config) -> Vec<String> {
    let own: Vec<_> = b
        .units
        .iter()
        .filter(|u| u.own && u.kind == "unit" && live(u))
        .collect();
    let map = b.collisions(c);
    let fields: Vec<_> = own
        .iter()
        .map(|u| {
            if u.ability == 10 {
                battle::distance_map(&map, u.cell(), u.speed, u.id)
            } else {
                None
            }
        })
        .collect();
    b.units
        .iter()
        .filter(|t| {
            live(t)
                && t.player_target()
                && (t.kind == "unit" && t.enemy
                    || matches!(t.kind.as_str(), "building" | "decor")
                        && t.appropriate
                        && !(3000..=3100).contains(&t.id)
                        && !matches!(t.id, 1026 | 1028))
        })
        .filter(|t| {
            own.iter().zip(&fields).any(|(u, field)| {
                if u.ability != 10 {
                    return u.distance_to(t) <= u.distance;
                }
                let Some(field) = field else { return false };
                let (w, h) = dimensions(t);
                let x0 = t.cell()[0] - w / 2 - 1;
                let y0 = t.cell()[1] - h / 2 - 1;
                let x1 = x0 + w + 1;
                let y1 = y0 + h + 1;
                (x0..=x1).any(|x| {
                    (y0..=y1).any(|y| {
                        if x != x0 && x != x1 && y != y0 && y != y1 {
                            return false;
                        }
                        if [x, y] == u.cell() {
                            return true;
                        }
                        if !(1..127).contains(&x) || !(1..127).contains(&y) {
                            return false;
                        }
                        let k = (x * 128 + y) as usize;
                        map[k] == 0 && field[k] > 0 && field[k] - 1 <= u.speed
                    })
                })
            })
        })
        .map(|t| t.uid.clone())
        .collect()
}
