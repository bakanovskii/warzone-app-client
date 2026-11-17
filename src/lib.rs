//! Standalone scene model. Dependencies are data/desktop libraries only.
//! Reuse the already verified pure state rules without compiling ARM adapters.
pub mod achievements;
pub mod animation;
pub mod bank;
pub mod battle;
pub mod battle_camera;
pub mod battle_motion;
pub mod battle_selection;
pub mod browser;
pub mod capacity;
pub mod construction;
pub mod construction_speedup;
pub mod entity_guid;
pub mod entity_state;
pub mod expansions;
pub mod hud;
pub mod interface;
pub mod interface_achievements;
pub mod interface_bank;
pub mod interface_battle;
pub mod interface_campaign;
pub mod interface_energy;
mod interface_lab;
pub mod interface_land;
pub mod interface_leagues;
pub mod interface_levelup;
pub mod interface_motion;
pub mod interface_pvp;
pub mod interface_resources;
pub mod interface_result;
pub mod interface_sale;
pub mod interface_shop;
pub mod interface_shortfall;
pub mod interface_story;
pub mod interface_text;
pub mod interface_unit_health;
pub mod interface_upgrades;
pub mod interface_windows;
pub mod inventory;
pub mod lifecycle;
pub mod locale;
pub mod mission;
pub mod obstacles;
pub mod presentation;
pub mod pricing;
pub mod production;
pub mod production_gather;
pub mod production_speedup;
pub mod pvp;
pub mod quest_navigation;
pub mod quest_story;
pub mod research;
pub mod resource_speedup;
pub mod reward_effects;
pub mod save;
pub mod shop;
pub mod sound;
pub mod story;
pub mod systems;
pub mod upgrade_gather;
pub mod upgrades;
pub mod workshop;
pub mod world;
pub mod world_markers;

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Projection {
    pub half_width: f64,
    pub half_height: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}
impl Projection {
    pub fn project(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.half_width * (x - y) + self.offset_x,
            self.half_height * (x + y) + self.offset_y,
        ]
    }
    pub fn inverse(&self, x: f64, y: f64) -> [f64; 2] {
        let a = (x - self.offset_x) / self.half_width;
        let b = (y - self.offset_y) / self.half_height;
        [(a + b) / 2., (b - a) / 2.]
    }
    /// Integer coordinates denote the top of a diamond; the whole visible
    /// diamond maps to [x,x+1)×[y,y+1), including its lower half.
    pub fn cell(&self, x: f64, y: f64) -> [i32; 2] {
        self.inverse(x, y).map(|v| (v + 1e-9).floor() as i32)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    /// SPTexture RGB uploads use GL_UNSIGNED_SHORT_4_4_4_4; UIKit images do not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sparrow_rgba4444: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sprite {
    pub src: [u32; 4],
    pub rect: [f64; 4],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainSprite {
    pub texture: String,
    pub src: [u32; 4],
    pub rect: [f64; 4],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub texture: String,
    pub fps: f64,
    pub frames: std::sync::Arc<[Vec<Sprite>]>,
    #[serde(skip)]
    pub ambient: Option<std::sync::Arc<animation::Clip>>,
}
impl Track {
    pub fn frame(&self, seconds: f64) -> animation::Frame<'_> {
        if let Some(clip) = &self.ambient {
            return animation::Frame::graph(clip, seconds);
        }
        let frame = if self.fps > 0. {
            (seconds.max(0.) * self.fps).floor() as usize % self.frames.len()
        } else {
            0
        };
        animation::Frame::Flat(self.frames[frame].iter().cloned())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub uid: String,
    pub type_id: u32,
    pub kind: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub direction: u32,
    pub width: f64,
    pub height: f64,
    pub archived: serde_json::Value,
    pub tracks: Vec<Track>,
}
impl Entity {
    pub fn footprint(&self) -> [f64; 4] {
        let (w, h) = match entity_state::direction_name(self.direction as i32) {
            "se" | "nw" => (self.width, self.height),
            _ => (self.height, self.width),
        };
        [self.x - (w / 2.).trunc(), self.y - (h / 2.).trunc(), w, h]
    }
    pub fn corners(&self, projection: &Projection) -> [[f64; 2]; 4] {
        let [x, y, w, h] = self.footprint();
        [
            projection.project(x - 0.5, y - 0.5),
            projection.project(x + w - 0.5, y - 0.5),
            projection.project(x + w - 0.5, y + h - 0.5),
            projection.project(x - 0.5, y + h - 0.5),
        ]
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    #[serde(default)]
    pub achievements: Option<achievements::Config>,
    #[serde(default)]
    pub obstacles: Option<obstacles::Config>,
    #[serde(default)]
    pub quest_navigation: Option<quest_navigation::Config>,
    #[serde(default)]
    pub expansions: Option<expansions::Config>,
    #[serde(default)]
    pub localization: Option<locale::Config>,
    #[serde(default)]
    pub upgrades: Option<upgrades::Config>,
    #[serde(default)]
    pub shop: Option<shop::Config>,
    #[serde(default)]
    pub interface: Option<interface::Config>,
    #[serde(default)]
    pub audio: Option<sound::Config>,
    pub format: String,
    pub projection: Projection,
    pub camera: [f64; 3],
    pub textures: HashMap<String, ImageInfo>,
    pub terrain: Vec<TerrainSprite>,
    #[serde(default)]
    pub ground_details: Vec<TerrainSprite>,
    #[serde(default)]
    pub ground_detail_layer: Option<usize>,
    pub entities: Vec<Entity>,
    pub resources: HashMap<String, i64>,
    #[serde(default)]
    pub economy: Option<production::Config>,
    #[serde(default)]
    pub progression: Option<systems::Config>,
    #[serde(default)]
    pub world: Option<world::Config>,
    #[serde(default)]
    pub construction: Option<construction::Config>,
    #[serde(default)]
    pub lifecycle: Option<lifecycle::Config>,
    #[serde(default)]
    pub workshop: Option<workshop::Config>,
    #[serde(default)]
    pub research: Option<research::Config>,
    #[serde(default)]
    pub pricing: Option<pricing::Config>,
    #[serde(default)]
    pub hud: Option<hud::Config>,
    #[serde(default)]
    pub combat: Option<battle::Config>,
    #[serde(default)]
    pub presentation: Option<presentation::Config>,
    pub font: String,
    pub provenance: serde_json::Value,
}
pub fn contained_file(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let p = Path::new(relative);
    if p.is_absolute()
        || p.components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!("Asset must use a relative pack path: {relative}"));
    }
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let path = base.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&base) {
        return Err("Asset symlink escapes pack".into());
    }
    Ok(path)
}
impl Scene {
    pub fn load(pack: &Path) -> Result<Self, String> {
        let raw = fs::read(contained_file(pack, "scene.json")?).map_err(|e| e.to_string())?;
        if raw.len() > 32 * 1024 * 1024 {
            return Err("Scene exceeds supported pack size".into());
        }
        let mut scene: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        scene.validate()?;
        animation::load(&mut scene, pack)?;
        scene
            .entities
            .sort_by(|a, b| world::depth_compare(a, b, &scene.projection));
        Ok(scene)
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(c) = &self.obstacles {
            c.validate(self)?;
        }
        if let Some(c) = &self.quest_navigation {
            c.validate(self)?;
        }
        if let Some(c) = &self.expansions {
            c.validate()?;
        }
        if let Some(c) = &self.localization {
            c.validate()?;
        }
        if let Some(upgrades) = &self.upgrades {
            upgrades.validate(self)?;
        }
        if let Some(shop) = &self.shop {
            shop.validate(self)?;
        }
        if self.format != "warzone-desktop-scene-v1" {
            return Err("Unsupported scene version".into());
        }
        if self.provenance["archive_coordinate_offset"].as_i64() != Some(16) {
            return Err("Scene must normalize archive coordinates to map space".into());
        }
        if self.entities.len() > 10000 || self.terrain.len() > 100000 {
            return Err("Unsupported scene counts".into());
        }
        if self.projection.half_width <= 0.
            || self.projection.half_height <= 0.
            || self.camera[2] <= 0.
        {
            return Err("Invalid projection/camera".into());
        }
        let mut ids = std::collections::HashSet::new();
        let valid = |texture: &str, src: [u32; 4], r: [f64; 4]| -> bool {
            let Some(t) = self.textures.get(texture) else {
                return false;
            };
            src[2] > 0
                && src[3] > 0
                && src[0].checked_add(src[2]).is_some_and(|v| v <= t.width)
                && src[1].checked_add(src[3]).is_some_and(|v| v <= t.height)
                && r.iter().all(|v| v.is_finite())
                && r[2] > 0.
                && r[3] > 0.
        };
        if self.ground_details.len() > 16384
            || self
                .ground_detail_layer
                .is_some_and(|i| i > self.terrain.len())
            || (!self.ground_details.is_empty() && self.ground_detail_layer.is_none())
        {
            return Err("Invalid ground detail layer".into());
        }
        for t in self.terrain.iter().chain(&self.ground_details) {
            if !valid(&t.texture, t.src, t.rect) {
                return Err("Invalid terrain region".into());
            }
        }
        for e in &self.entities {
            if !ids.insert(&e.uid)
                || !e.x.is_finite()
                || !e.y.is_finite()
                || e.width <= 0.
                || e.height <= 0.
            {
                return Err("Invalid entity identity/position/footprint".into());
            }
            if e.tracks.is_empty() {
                return Err(format!("Unresolved entity {}", e.uid));
            }
            for t in &e.tracks {
                if t.frames.is_empty() || !t.fps.is_finite() || t.fps < 0. {
                    return Err("Invalid animation".into());
                }
                for f in t.frames.iter() {
                    for s in f {
                        if !valid(&t.texture, s.src, s.rect) {
                            return Err("Invalid animation region".into());
                        }
                    }
                }
            }
        }
        if let Some(world) = &self.world {
            world.validate(self)?;
        }
        if let Some(hud) = &self.hud {
            hud.validate(self)?;
        }
        if let Some(combat) = &self.combat {
            combat.validate()?;
            combat.validate_art(self)?;
        }
        if let Some(p) = &self.presentation {
            p.validate(self)?;
        }
        if let Some(audio) = &self.audio {
            audio.validate()?;
        }
        if let Some(p) = &self.pricing {
            p.validate()?;
        }
        if let Some(c) = &self.achievements {
            c.validate()?;
        }
        if let Some(ui) = &self.interface {
            ui.validate(self)?;
        }
        Ok(())
    }
    pub fn entity_in_visible_world(&self, entity: &Entity) -> bool {
        self.world.as_ref().is_none_or(|world| {
            let [x, y, w, h] = world.visible_rect;
            entity.x >= x && entity.y >= y && entity.x < x + w && entity.y < y + h
        })
    }
    pub fn pick(&self, camera: &Camera, screen: [f64; 2]) -> Option<usize> {
        self.pick_filtered(camera, screen, |_| true)
    }
    pub fn pick_filtered(
        &self,
        camera: &Camera,
        screen: [f64; 2],
        accept: impl Fn(&Entity) -> bool,
    ) -> Option<usize> {
        self.pick_filtered_positions(camera, screen, accept, |e| [e.x, e.y])
    }
    /// A committed battle stores final coordinates while its object-local
    /// presentation queue may still be moving. Pick that visible pose without
    /// cloning the scene, textures or immutable animation tracks per input.
    pub fn pick_filtered_positions(
        &self,
        camera: &Camera,
        screen: [f64; 2],
        accept: impl Fn(&Entity) -> bool,
        position: impl Fn(&Entity) -> [f64; 2],
    ) -> Option<usize> {
        self.pick_positions(camera, screen, accept, position, false)
    }
    /// Battle cells start at integer coordinates. Keep picking independent of
    /// whether this view retains the source catalogs.
    pub fn pick_battle_filtered_positions(
        &self,
        camera: &Camera,
        screen: [f64; 2],
        accept: impl Fn(&Entity) -> bool,
        position: impl Fn(&Entity) -> [f64; 2],
    ) -> Option<usize> {
        self.pick_positions(camera, screen, accept, position, true)
    }
    fn pick_positions(
        &self,
        camera: &Camera,
        screen: [f64; 2],
        accept: impl Fn(&Entity) -> bool,
        position: impl Fn(&Entity) -> [f64; 2],
        battle: bool,
    ) -> Option<usize> {
        let w = camera.world(screen);
        let p = self.projection.inverse(w[0], w[1]);
        let picked = self.entities.iter().enumerate().rev().find_map(|(i, e)| {
            if !accept(e) || !self.entity_in_visible_world(e) {
                return None;
            }
            let at = position(e);
            let anchor = self.projection.project(at[0], at[1]);
            let local = [w[0] - anchor[0], w[1] - anchor[1]];
            // Unit.createSprite: animationY35 plus SPAnimation's authored
            // ellipse(-40,-60,80,80). Collision footprints are not touch areas.
            if e.kind == "unit" || e.kind == "units" {
                return (((local[0] / 40.).powi(2) + ((local[1] - 15.) / 40.).powi(2)) <= 1.)
                    .then_some(i);
            }
            if e.archived["state"].as_i64() == Some(3)
                && let Some(shapes) = self
                    .world
                    .as_ref()
                    .and_then(|c| c.hit_areas.get(&e.type_id))
                    .and_then(|v| v.get(&e.direction))
                    .filter(|v| !v.is_empty())
            {
                return shapes.iter().any(|a| a.contains(local)).then_some(i);
            }
            // CJSONDraw marks its bitmap children non-touchable. When no
            // authored shape accepts the tap, SceneMain uses its tile lookup;
            // transparent fence textures must not steal neighboring unit taps.
            let [x, y, width, height] = e.footprint();
            let (x, y) = (x + at[0] - e.x, y + at[1] - e.y);
            let margin = if battle || self.world.is_none() && self.combat.is_some() {
                0.
            } else {
                0.5
            };
            (p[0] >= x - margin
                && p[0] < x + width - margin
                && p[1] >= y - margin
                && p[1] < y + height - margin)
                .then_some(i)
        })?;
        let field = &self.entities[picked];
        if [1026, 1028].contains(&field.type_id) {
            // SceneMain.getItemBySprite -> getHarvesterFromPlace: same tile.
            self.entities
                .iter()
                .position(|e| e.type_id == 1027 && e.x == field.x && e.y == field.y)
        } else {
            Some(picked)
        }
    }
    pub fn item_name<'a>(&'a self, id: u32, fallback: &'a str) -> &'a str {
        self.progression
            .as_ref()
            .and_then(|c| c.language.get(&format!("#item_{id}_name")))
            .map(String::as_str)
            .unwrap_or(fallback)
    }
}
#[derive(Clone, Debug)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub width: u32,
    pub height: u32,
}
impl Camera {
    pub fn saved_pose(&self) -> [f64; 3] {
        [self.x, self.y, self.zoom / crate::browser::zoom_factor()]
    }
    pub fn constrain(&mut self, scene: &Scene) {
        if let Some(world) = &scene.world {
            let [x, y, w, h] = world.visible_rect;
            let p = scene.projection.inverse(self.x, self.y);
            let p = scene
                .projection
                .project(p[0].clamp(x, x + w), p[1].clamp(y, y + h));
            self.x = p[0];
            self.y = p[1];
        }
    }
    pub fn screen(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[0] - self.x) * self.zoom + self.width as f64 / 2.,
            (p[1] - self.y) * self.zoom + self.height as f64 / 2.,
        ]
    }
    pub fn world(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[0] - self.width as f64 / 2.) / self.zoom + self.x,
            (p[1] - self.height as f64 / 2.) / self.zoom + self.y,
        ]
    }
    pub fn zoom_at(&mut self, factor: f64, p: [f64; 2]) {
        let before = self.world(p);
        let density = crate::browser::zoom_factor();
        self.zoom = (self.zoom * factor).clamp(0.12 * density, 2. * density);
        let after = self.world(p);
        self.x += before[0] - after[0];
        self.y += before[1] - after[1];
    }
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.x -= dx / self.zoom;
        self.y -= dy / self.zoom;
    }
}

pub mod callback_clock;

pub mod production_shortfall;
pub mod scroll_motion;
