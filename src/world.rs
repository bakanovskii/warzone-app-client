//! Native base layout: original expansion grid, footprints and edit transactions.
//! SceneMain.prepareCollisionsData/isIntersect, SceneObject direction cycle and
//! UserProfile.setItemPosition are recovered rules; no guest calls or RPC.
use crate::{
    Entity, Scene, Track,
    production::{Action, State, number},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SE: u32 = 0x100000;
pub const SW: u32 = 0x200000;
pub const NE: u32 = 0x400000;
pub const NW: u32 = 0x800000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub kind: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub variants: BTreeMap<u32, Vec<Track>>,
    #[serde(default)]
    pub stages: BTreeMap<u32, BTreeMap<u32, Vec<Track>>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HitArea {
    pub rect: [f64; 4],
    pub ellipse: bool,
}
impl HitArea {
    pub fn contains(&self, p: [f64; 2]) -> bool {
        let [x, y, w, h] = self.rect;
        if self.ellipse {
            ((p[0] - x - w / 2.) / (w / 2.)).powi(2) + ((p[1] - y - h / 2.) / (h / 2.)).powi(2) < 1.
        } else {
            p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Original defaultmap.project fog rectangle, converted from archive to
    /// world coordinates. Do not draw scenery beyond the black world edge.
    #[serde(default = "source_visible_rect")]
    pub visible_rect: [f64; 4],
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hit_areas: BTreeMap<u32, BTreeMap<u32, Vec<HitArea>>>,
    pub grid_size: usize,
    /// Indexed [archive x][archive y], exactly Landscape.nMapData, not screen rows.
    pub water: Vec<String>,
    pub catalog: BTreeMap<u32, Definition>,
    #[serde(default)]
    pub fences: BTreeMap<u32, BTreeMap<u32, Vec<Track>>>,
    #[serde(default)]
    pub working: BTreeMap<u32, Working>,
}
fn source_visible_rect() -> [f64; 4] {
    [16., 10., 74., 68.]
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Working {
    pub normal: BTreeMap<u32, Vec<Track>>,
    #[serde(default)]
    pub alternate: BTreeMap<u32, Vec<Track>>,
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.grid_size != 128
            || self.water.len() != 128
            || self
                .water
                .iter()
                .any(|r| r.len() != 128 || !r.bytes().all(|b| b == b'0' || b == b'1'))
        {
            return Err("Invalid native world grid".into());
        }
        for variants in self.hit_areas.values() {
            for shapes in variants.values() {
                if shapes.len() > 32
                    || shapes.iter().any(|s| {
                        s.rect.iter().any(|v| !v.is_finite()) || s.rect[2] <= 0. || s.rect[3] <= 0.
                    })
                {
                    return Err("Invalid source hit-test shapes".into());
                }
            }
        }
        // Reuse the same atlas bounds checks as initial instance visuals.
        let mut visuals = scene.clone();
        visuals.world = None;
        // This recursive pass validates sprite tracks only; the full catalog
        // and its shop references were checked against the real scene above.
        visuals.shop = None;
        visuals.upgrades = None;
        visuals.entities.clear();
        for (&id, d) in &self.catalog {
            if !d.width.is_finite()
                || !d.height.is_finite()
                || !(1.0..=32.0).contains(&d.width)
                || !(1.0..=32.0).contains(&d.height)
                || d.width.fract() != 0.0
                || d.height.fract() != 0.0
            {
                return Err("Invalid catalog footprint".into());
            }
            for (&direction, tracks) in &d.variants {
                if ![SE, SW, NE, NW].contains(&direction) {
                    return Err("Invalid catalog direction".into());
                }
                visuals.entities.push(Entity {
                    uid: format!("{id}:{direction}"),
                    type_id: id,
                    kind: d.kind.clone(),
                    name: d.name.clone(),
                    x: 0.0,
                    y: 0.0,
                    direction,
                    width: d.width,
                    height: d.height,
                    archived: serde_json::Value::Null,
                    tracks: tracks.clone(),
                });
            }
            for (&stage, variants) in &d.stages {
                if ![0, 1, 2].contains(&stage) {
                    return Err("Invalid construction visual stage".into());
                }
                for (&direction, tracks) in variants {
                    visuals.entities.push(Entity {
                        uid: format!("{id}:{stage}:{direction}"),
                        type_id: id,
                        kind: d.kind.clone(),
                        name: d.name.clone(),
                        x: 0.0,
                        y: 0.0,
                        direction,
                        width: d.width,
                        height: d.height,
                        archived: serde_json::Value::Null,
                        tracks: tracks.clone(),
                    });
                }
            }
        }
        for (&id, variants) in &self.fences {
            if ![3001, 3002, 3010, 3011].contains(&id) || variants.len() != 11 {
                return Err("Invalid fence variants".into());
            }
            for (&variant, tracks) in variants {
                if !(1..=11).contains(&variant) {
                    return Err("Invalid fence type".into());
                }
                visuals.entities.push(Entity {
                    uid: format!("fence:{id}:{variant}"),
                    type_id: id,
                    kind: "decor".into(),
                    name: String::new(),
                    x: 0.,
                    y: 0.,
                    direction: SE,
                    width: 1.,
                    height: 1.,
                    archived: serde_json::Value::Null,
                    tracks: tracks.clone(),
                });
            }
        }
        for (&id, work) in &self.working {
            let def = self
                .catalog
                .get(&id)
                .filter(|d| d.kind == "buildings")
                .ok_or("Unknown working building")?;
            if work.normal.is_empty() || (!work.alternate.is_empty() && id != 1027) {
                return Err("Invalid working building variants".into());
            }
            for (&direction, tracks) in work.normal.iter().chain(work.alternate.iter()) {
                if ![SE, SW, NE, NW].contains(&direction) || tracks.is_empty() {
                    return Err("Invalid production direction".into());
                }
                visuals.entities.push(Entity {
                    uid: format!("work:{id}:{direction}:{}", visuals.entities.len()),
                    type_id: id,
                    kind: "buildings".into(),
                    name: String::new(),
                    x: 0.,
                    y: 0.,
                    direction,
                    width: def.width,
                    height: def.height,
                    archived: serde_json::Value::Null,
                    tracks: tracks.clone(),
                });
            }
        }
        visuals.validate()?;
        for e in &scene.entities {
            let d = self.catalog.get(&e.type_id).ok_or("Missing catalog type")?;
            if d.kind != e.kind || d.width != e.width || d.height != e.height {
                return Err("Seed and catalog definitions disagree".into());
            }
        }
        Ok(())
    }
}

/// UserProfile.getExpansionPositionX/Y: six by six tiles in a diamond spiral.
pub fn expansion_position(id: i32) -> Result<[i32; 2], String> {
    if !(1..=32768).contains(&id) {
        return Err("Invalid expansion ID".into());
    }
    if id == 1 {
        return Ok([0, 0]);
    }
    let k = id - 1;
    let n = ((((2 * k - 1) as f64).sqrt() + 1.0) * 0.5) as f32;
    let n = n.floor() as i32;
    let off = k - ((2 * n * (n - 1)) | 1);
    let x = if off <= 2 * n {
        if off > n { 2 * n - off } else { off }
    } else if off > 3 * n {
        off - 4 * n
    } else {
        2 * n - off
    };
    let y = if off <= 2 * n { off - n } else { 3 * n - off };
    Ok([x, y])
}
pub fn next_direction(id: u32, direction: u32) -> u32 {
    // Units and buildings 1021/1022/1036 have four directions. Others flip SE/SW.
    if !(2000..=2998).contains(&id) && ![1021, 1022, 1036].contains(&id) {
        return if direction == SW { SE } else { SW };
    }
    match direction {
        NE => NW,
        SW => SE,
        SE => NE,
        _ => SW,
    }
}
fn facing_allowed(id: u32, original: u32, requested: u32) -> bool {
    if original == requested {
        return true;
    }
    let mut facing = original;
    for _ in 0..4 {
        facing = next_direction(id, facing);
        if facing == requested {
            return true;
        }
    }
    false
}
fn position(v: &serde_json::Value) -> Result<[i32; 2], String> {
    let get = |k: &str| {
        v[k].as_f64()
            .filter(|n| n.is_finite() && n.fract() == 0.0 && (0.0..128.0).contains(n))
            .map(|n| n as i32)
            .ok_or_else(|| "Invalid archived world coordinate".to_string())
    };
    Ok([get("x")?, get("y")?])
}
fn footprint(d: &Definition, p: [i32; 2], direction: u32) -> [i32; 4] {
    let (w, h) = if [SW, NE].contains(&direction) {
        (d.height as i32, d.width as i32)
    } else {
        (d.width as i32, d.height as i32)
    };
    [p[0] - w / 2, p[1] - h / 2, w, h]
}

/// SceneObject.getCenterPoint averages the four projected footprint corners.
/// Even dimensions shift their center by half a cell. SceneMain.sortAllObjects
/// compares screen Y, then X, then type (crystal1028 is treated as1026).
pub fn depth_key(entity: &Entity, projection: &crate::Projection) -> [f64; 3] {
    depth_key_at(entity, projection, entity.x, entity.y)
}
pub fn depth_key_at(entity: &Entity, projection: &crate::Projection, x: f64, y: f64) -> [f64; 3] {
    let (w, h) = if [SW, NE].contains(&entity.direction) {
        (entity.height as i32, entity.width as i32)
    } else {
        (entity.width as i32, entity.height as i32)
    };
    let p = projection.project(
        x - if w % 2 == 0 { 0.5 } else { 0. },
        y - if h % 2 == 0 { 0.5 } else { 0. },
    );
    [
        p[1],
        p[0],
        if entity.type_id == 1028 {
            1026.
        } else {
            entity.type_id as f64
        },
    ]
}
pub fn depth_compare(a: &Entity, b: &Entity, p: &crate::Projection) -> std::cmp::Ordering {
    let a = depth_key(a, p);
    let b = depth_key(b, p);
    a[0].total_cmp(&b[0])
        .then_with(|| a[1].total_cmp(&b[1]))
        .then_with(|| a[2].total_cmp(&b[2]))
}

/// Production SPAnimation.restart is transient; it must not change the save.
#[derive(Default)]
pub struct WorkingPlayback {
    starts: BTreeMap<String, ([i64; 3], f64)>,
}
impl WorkingPlayback {
    pub fn clock(&mut self, scene: &Scene, entity: &Entity, track: &Track, seconds: f64) -> f64 {
        let Some(work) = scene
            .world
            .as_ref()
            .and_then(|c| c.working.get(&entity.type_id))
        else {
            return seconds;
        };
        let production = number(&entity.archived, "productionType");
        if production == 0
            || !work
                .normal
                .values()
                .chain(work.alternate.values())
                .flatten()
                .any(|t| t.texture == track.texture)
        {
            return seconds;
        }
        let signature = [
            production,
            number(&entity.archived, "productionStartTime"),
            entity.direction as i64,
        ];
        let entry = self
            .starts
            .entry(entity.uid.clone())
            .or_insert((signature, seconds));
        if entry.0 != signature {
            *entry = (signature, seconds);
        }
        (seconds - entry.1).max(0.)
    }
}
#[derive(Clone, Debug)]
pub struct LayoutEdit {
    pub uid: String,
    /// Map/wire coordinates; the archive offset is applied exactly once.
    pub x: i32,
    pub y: i32,
    pub direction: u32,
    pub rotation_only: bool,
}
impl State {
    pub fn can_edit_world(&self, scene: &Scene, uid: &str, rotate: bool) -> Result<(), String> {
        if self.gathering(uid) {
            return Err("Дождись завершения сбора".into());
        }
        let cfg = scene
            .world
            .as_ref()
            .ok_or("Перемещение недоступно в этом пакете")?;
        let v = self.entities.get(uid).ok_or("Объект не найден")?;
        let id = number(v, "type") as u32;
        let d = cfg.catalog.get(&id).ok_or("Неизвестный тип объекта")?;
        if d.kind == "obstacles"
            || [1026, 1027, 1028].contains(&id)
            || v["isDeleted"] == true
            || v["isInBattle"] == true
            || (d.kind == "buildings" && number(v, "state") != 3)
        {
            return Err("Этот объект сейчас нельзя перемещать".into());
        }
        // Fence orientation is connectivity, not a facing direction.
        if rotate && [3001, 3002, 3010, 3011].contains(&id) {
            return Err("Направление ограды определяется соседями".into());
        }
        Ok(())
    }
    pub fn begin_layout(
        &self,
        scene: &Scene,
        uid: &str,
        rotate: bool,
    ) -> Result<LayoutEdit, String> {
        self.can_edit_world(scene, uid, rotate)?;
        let v = &self.entities[uid];
        let [x, y] = position(v)?;
        let direction = number(v, "direction") as u32;
        Ok(LayoutEdit {
            uid: uid.into(),
            x: x - 16,
            y: y - 16,
            direction: if rotate {
                next_direction(number(v, "type") as u32, direction)
            } else {
                direction
            },
            rotation_only: rotate,
        })
    }
    pub fn placement(&self, scene: &Scene, edit: &LayoutEdit) -> Result<(), String> {
        self.can_edit_world(scene, &edit.uid, edit.rotation_only)?;
        let cfg = scene.world.as_ref().unwrap();
        let v = &self.entities[&edit.uid];
        let id = number(v, "type") as u32;
        let original = number(v, "direction") as u32;
        if edit.direction != original {
            self.can_edit_world(scene, &edit.uid, true)?;
            if !facing_allowed(id, original, edit.direction) {
                return Err("Недопустимое направление объекта".into());
            }
        }
        if edit.direction != original && !cfg.catalog[&id].variants.contains_key(&edit.direction) {
            return Err("Нет исходной графики для этого направления".into());
        }
        if edit.rotation_only && position(v)? != [edit.x + 16, edit.y + 16] {
            return Err("Поворот не меняет положение объекта".into());
        }
        self.placement_type(scene, id, &edit.uid, edit.x, edit.y, edit.direction)
    }
    pub fn placement_type(
        &self,
        scene: &Scene,
        id: u32,
        ignore: &str,
        px: i32,
        py: i32,
        direction: u32,
    ) -> Result<(), String> {
        let cfg = scene.world.as_ref().ok_or("Missing world catalog")?;
        if !cfg.catalog.contains_key(&id) {
            return Err("Unknown placement type".into());
        }
        if !(-16..112).contains(&px) || !(-16..112).contains(&py) {
            return Err("За пределами карты".into());
        }
        let [x, y, w, h] = footprint(&cfg.catalog[&id], [px + 16, py + 16], direction);
        if id == 1027
            && !self.entities.values().any(|e| {
                [1026, 1028].contains(&(number(e, "type") as u32))
                    && e["isDeleted"] != true
                    && number(e, "x") == px as i64 + 16
                    && number(e, "y") == py as i64 + 16
            })
        {
            return Err("Харвестер можно разместить только на поле кристаллов".into());
        }
        let mut owned = [[false; 128]; 128];
        let preferences = &self
            .systems
            .as_ref()
            .ok_or("Missing world preferences")?
            .preferences;
        let expansions = preferences
            .get("expansionsMap")
            .and_then(|v| v.as_object())
            .ok_or("Missing owned land")?;
        for key in expansions.keys() {
            let p = expansion_position(key.parse().map_err(|_| "Invalid expansion key")?)?;
            for row in p[0] * 6 + 16..p[0] * 6 + 22 {
                for col in p[1] * 6 + 16..p[1] * 6 + 22 {
                    if (0..128).contains(&row) && (0..128).contains(&col) {
                        owned[row as usize][col as usize] = true;
                    }
                }
            }
        }
        for a in x..x + w {
            for b in y..y + h {
                if !(0..128).contains(&a)
                    || !(0..128).contains(&b)
                    || !owned[a as usize][b as usize]
                {
                    return Err("Нужно купить этот участок земли".into());
                }
                if cfg.water[a as usize].as_bytes()[b as usize] == b'1' {
                    return Err("Здесь вода".into());
                }
            }
        }
        for (uid, other) in &self.entities {
            if uid == ignore || other["isDeleted"] == true {
                continue;
            }
            if id == 1027
                && [1026, 1028].contains(&(number(other, "type") as u32))
                && number(other, "x") == px as i64 + 16
                && number(other, "y") == py as i64 + 16
            {
                continue;
            }
            let d = cfg
                .catalog
                .get(&(number(other, "type") as u32))
                .ok_or("Missing collision type")?;
            let [a, b, c, d] = footprint(d, position(other)?, number(other, "direction") as u32);
            if x < a + c && a < x + w && y < b + d && b < y + h {
                return Err("Место занято другим объектом".into());
            }
        }
        Ok(())
    }
    pub fn apply_layout(
        &self,
        scene: &Scene,
        edit: &LayoutEdit,
        wall: i64,
    ) -> Result<(Self, String), String> {
        self.placement(scene, edit)?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let v = &self.entities[&edit.uid];
        if position(v)? == [edit.x + 16, edit.y + 16]
            && number(v, "direction") == edit.direction as i64
        {
            return Err("Положение объекта не изменилось".into());
        }
        let mut next = self.clone();
        let id = number(v, "type") as u32;
        let v = next.entities.get_mut(&edit.uid).unwrap();
        v["x"] = (edit.x + 16).into();
        v["y"] = (edit.y + 16).into();
        v["direction"] = edit.direction.into();
        let level = next.resources["level"];
        if let Some(c) = &scene.progression {
            next.systems.as_mut().ok_or("Missing quest state")?.event(
                c,
                level,
                if edit.rotation_only { 11 } else { 1 },
                id,
            )?;
        }
        next.revision = next.revision.checked_add(1).ok_or("Revision overflow")?;
        next.last_action_time = now;
        next.systems.as_mut().unwrap().applied_action_revision = next.revision;
        next.actions.push(Action {
            at_ms: None,
            shop: None,
            source: None,
            sequence: next.revision,
            at: now,
            uid: edit.uid.clone(),
            recipe: id,
            kind: if edit.rotation_only {
                "world_rotate"
            } else {
                "world_move"
            }
            .into(),
            gains: BTreeMap::new(),
            entropy: vec![],
            quest_events_pending: vec![],
        });
        next.selected = Some(edit.uid.clone());
        next.sync_preferences();
        next.validate(scene)?;
        Ok((
            next,
            if edit.rotation_only {
                "Объект повёрнут"
            } else {
                "Объект перемещён"
            }
            .into(),
        ))
    }
    pub fn validate_world(&self, scene: &Scene) -> Result<(), String> {
        for seed in &scene.entities {
            let v = &self.entities[&seed.uid];
            position(v)?;
            if v["x"] != seed.archived["x"]
                || v["y"] != seed.archived["y"]
                || v["direction"] != seed.archived["direction"]
            {
                let p = position(v)?;
                let direction = v["direction"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("Invalid saved direction")?;
                if v["isDeleted"] != true {
                    self.placement_type(
                        scene,
                        seed.type_id,
                        &seed.uid,
                        p[0] - 16,
                        p[1] - 16,
                        direction,
                    )?;
                }
                // A moved fence may retain its original connectivity mask.
                if direction != seed.direction && ![SE, SW, NE, NW].contains(&direction) {
                    return Err("Invalid saved facing".into());
                }
                if !facing_allowed(seed.type_id, seed.direction, direction) {
                    return Err("Invalid saved direction for this type".into());
                }
                if seed.kind == "obstacles"
                    || [1026, 1027, 1028].contains(&seed.type_id)
                    || (seed.kind == "decor" && direction != seed.direction)
                {
                    return Err("Saved pose changes a fixed object".into());
                }
                if direction != seed.direction
                    && !scene.world.as_ref().unwrap().catalog[&seed.type_id]
                        .variants
                        .contains_key(&direction)
                {
                    return Err("Missing saved facing artwork".into());
                }
            }
        }
        Ok(())
    }
    pub fn world_entities(&self, scene: &Scene) -> Result<Vec<Entity>, String> {
        self.world_entities_at(scene, None)
    }
    pub fn world_entities_at(
        &self,
        scene: &Scene,
        now: Option<i64>,
    ) -> Result<Vec<Entity>, String> {
        let mut result = Vec::with_capacity(self.entities.len());
        for (uid, v) in &self.entities {
            if v["isDeleted"] == true || v["isInBattle"] == true {
                continue;
            }
            let seed = scene.entities.iter().find(|e| &e.uid == uid);
            let mut e = if let Some(seed) = seed {
                let mut e = seed.clone();
                let [x, y] = position(v)?;
                e.x = (x - 16) as f64;
                e.y = (y - 16) as f64;
                e.direction = number(v, "direction") as u32;
                e.archived = v.clone();
                if e.kind == "buildings"
                    && (v["state"] != seed.archived["state"]
                        || now.is_some() && matches!(number(v, "state"), 1 | 2))
                {
                    e.tracks = self.catalog_entity(scene, v, now)?.tracks;
                } else if e.direction != seed.direction {
                    e.tracks = scene
                        .world
                        .as_ref()
                        .and_then(|c| c.catalog.get(&e.type_id))
                        .and_then(|d| d.variants.get(&e.direction))
                        .ok_or("Missing saved facing artwork")?
                        .clone();
                }
                if let Some(upgraded) = self.upgraded_tracks(scene, e.type_id, e.direction) {
                    e.tracks = upgraded.clone();
                }
                e
            } else {
                self.catalog_entity(scene, v, now)?
            };
            if let (Some(now), Some(work)) = (
                now,
                scene.world.as_ref().and_then(|c| c.working.get(&e.type_id)),
            ) {
                let id = number(v, "productionType") as u32;
                let started = number(v, "productionStartTime");
                let duration = if e.type_id == 1019 {
                    scene
                        .upgrades
                        .as_ref()
                        .and_then(|c| c.items.get(&id))
                        .map(|r| r.seconds)
                } else if e.type_id == 1020 {
                    scene
                        .research
                        .as_ref()
                        .and_then(|c| c.items.get(&id))
                        .map(|r| r.seconds)
                } else if let Some(r) = scene.workshop.as_ref().and_then(|c| c.recipes.get(&id)) {
                    let seq = self.workshop_origin_sequence(uid, id, started, u64::MAX);
                    Some(self.upgraded_recipe(scene, r, seq).seconds)
                } else {
                    scene
                        .economy
                        .as_ref()
                        .and_then(|c| c.recipes.get(&id))
                        .map(|r| r.seconds)
                };
                if number(v, "state") == 3
                    && !self.gathering(uid)
                    && duration.is_some_and(|seconds| {
                        started > 0 && now >= started && now <= started.saturating_add(seconds)
                    })
                {
                    let alternate = e.type_id == 1027 && id == 1016;
                    let clips = if alternate {
                        &work.alternate
                    } else {
                        &work.normal
                    };
                    if let Some(tracks) = clips.get(&e.direction) {
                        if !alternate {
                            e.tracks.retain(|t| t.fps == 0.);
                        }
                        e.tracks.extend(tracks.clone());
                    }
                }
            }
            result.push(e);
        }
        if let Some(c) = &scene.world {
            let fences: std::collections::BTreeSet<_> = result
                .iter()
                .filter(|e| [3001, 3002, 3010, 3011].contains(&e.type_id))
                .map(|e| (e.x as i32, e.y as i32))
                .collect();
            for e in &mut result {
                if let Some(art) = c.fences.get(&e.type_id) {
                    let (x, y) = (e.x as i32, e.y as i32);
                    let mask = [(x - 1, y), (x, y - 1), (x + 1, y), (x, y + 1)]
                        .iter()
                        .fold(0, |bits, p| bits * 2 + usize::from(fences.contains(p)));
                    const TYPES: [u32; 16] = [11, 3, 7, 10, 3, 3, 2, 1, 7, 9, 7, 8, 6, 5, 4, 11];
                    e.tracks = art
                        .get(&TYPES[mask])
                        .ok_or("Missing connected fence art")?
                        .clone();
                }
            }
        }
        // Preserve imported order. New objects are appended by the source
        // placement handlers before sortAllObjects; their random GUID is not a
        // depth criterion. The first durable receipt recovers that append order
        // on reload without introducing a second saved list.
        let order: std::collections::HashMap<_, _> = scene
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| (e.uid.as_str(), i))
            .collect();
        let mut created = std::collections::HashMap::new();
        for a in &self.actions {
            if !order.contains_key(a.uid.as_str()) {
                created.entry(a.uid.as_str()).or_insert(a.sequence);
            }
        }
        result.sort_by(|a, b| {
            depth_compare(a, b, &scene.projection)
                .then_with(|| {
                    order
                        .get(a.uid.as_str())
                        .copied()
                        .unwrap_or(usize::MAX)
                        .cmp(&order.get(b.uid.as_str()).copied().unwrap_or(usize::MAX))
                })
                .then_with(|| {
                    if !order.contains_key(a.uid.as_str()) && !order.contains_key(b.uid.as_str()) {
                        created
                            .get(a.uid.as_str())
                            .copied()
                            .unwrap_or(u64::MAX)
                            .cmp(&created.get(b.uid.as_str()).copied().unwrap_or(u64::MAX))
                            .then(a.uid.cmp(&b.uid))
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
        });
        Ok(result)
    }
}
