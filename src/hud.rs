//! Original top_hud resource artwork and layout. Native dialogs are separate.
use crate::{Camera, Scene, production::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub shadow: Option<crate::interface::TextEffect>,
    #[serde(default)]
    pub glow: Option<crate::interface::TextEffect>,
    pub kind: String,
    pub texture: String,
    pub rect: [f64; 4],
    pub binding: String,
    pub color: u32,
    pub font_size: f64,
    pub bold: bool,
    pub align: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub logical_width: f64,
    pub nodes: Vec<Node>,
    pub bold_font: String,
    pub bold_font_sha256: String,
}
#[derive(Default)]
pub struct Values {
    pub texts: BTreeMap<String, String>,
    pub progress: BTreeMap<String, f64>,
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.logical_width != 480.0 || self.nodes.len() > 256 {
            return Err("Invalid HUD layout".into());
        }
        for n in &self.nodes {
            if !n.rect.iter().all(|v| v.is_finite())
                || n.rect[2] <= 0.0
                || n.rect[3] <= 0.0
                || n.color > 0xffffff
                || !["image", "text"].contains(&n.kind.as_str())
            {
                return Err("Invalid HUD node".into());
            }
            if n.kind == "image" && !scene.textures.contains_key(&n.texture) {
                return Err("Missing original HUD texture".into());
            }
            if n.kind == "text"
                && (!(1.0..=40.0).contains(&n.font_size)
                    || !["left", "right", "center"].contains(&n.align.as_str()))
            {
                return Err("Invalid HUD type style".into());
            }
        }
        Ok(())
    }
}
pub fn scale(camera: &Camera) -> f64 {
    scale_at_density(camera, crate::browser::display_density())
}
fn scale_at_density(camera: &Camera, density: f64) -> f64 {
    (camera.width as f64 / 480.0)
        .min(camera.height as f64 / 320.0)
        .clamp(0.5 * density, 2.5 * density)
}
/// Only the authored full-width background stretches. Resource artwork, labels
/// and hit targets keep the same scale; tall/ultrawide windows expose more map.
pub fn image_rect(node: &Node, camera: &Camera) -> [f64; 4] {
    let s = scale(camera);
    let [x, y, w, h] = node.rect;
    let width = if node.binding.is_empty() && x <= 0. && y == 0. && w >= 480. && h <= 32. {
        (w * s).max(camera.width as f64 - x * s)
    } else {
        w * s
    };
    [x * s, y * s, width, h * s]
}
pub fn height(scene: &Scene, camera: &Camera) -> i32 {
    if scene.hud.is_some() {
        (62.0 * scale(camera)).ceil() as i32
    } else {
        78
    }
}
pub fn blocks_point(scene: &Scene, camera: &Camera, x: i32, y: i32) -> bool {
    if scene.interface.is_some() {
        let s = scale(camera);
        y >= 0 && ((y as f64) < 32. * s || ((x as f64) < 85. * s && (y as f64) < 60. * s))
    } else {
        y >= 0 && y < height(scene, camera)
    }
}
pub fn bottom_reserved(scene: &Scene) -> i32 {
    if scene.interface.is_some() { 0 } else { 138 }
}
/// x,y,width,height for the four native navigation actions in the spare second row.
pub fn navigation(scene: &Scene, camera: &Camera) -> Vec<[i32; 4]> {
    if scene.hud.is_none() {
        return vec![
            [12, 40, 148, 38],
            [camera.width as i32 - 450, 40, 150, 38],
            [camera.width as i32 - 300, 40, 155, 38],
            [camera.width as i32 - 145, 40, 145, 38],
        ];
    }
    let s = scale(camera);
    let x = (86.0 * s).ceil() as i32;
    let y = (32.0 * s).ceil() as i32;
    let w = (camera.width as i32 - x) / 4;
    (0..4)
        .map(|i| [x + i * w, y, w, (25.0 * s).ceil() as i32])
        .collect()
}
impl State {
    pub fn hud_values(&self, scene: &Scene, wall: i64) -> Values {
        let mut v = Values::default();
        for k in ["level", "exp", "playMoney", "realMoney", "fuel", "iron"] {
            v.texts.insert(
                k.into(),
                self.resources.get(k).copied().unwrap_or(0).to_string(),
            );
        }
        let now = scene
            .economy
            .as_ref()
            .and_then(|c| self.clock(c, wall).ok())
            .unwrap_or(wall);
        let energy = self.energy(scene, now);
        let max = self.max_energy(scene);
        v.texts.insert("energy".into(), format!("{energy}/{max}"));
        v.progress.insert(
            "energy_progress".into(),
            if max > 0 {
                (energy as f64 / max as f64).clamp(0.0, 1.0)
            } else {
                0.0
            },
        );
        let timer = if energy < max {
            let left = self.energy_next_seconds(scene, now);
            format!("{}:{:02}", left / 60, left % 60)
        } else {
            String::new()
        };
        v.texts.insert("energy_timer".into(), timer);
        let honor = self.systems.as_ref().map_or(0, |s| s.integer("honor"));
        v.texts.insert("honor".into(), honor.to_string());
        // CHUDLayer: (exp-getMinExperience)/(getMaxExperience-getMinExperience).
        let (min, max) = scene
            .progression
            .as_ref()
            .map(|c| {
                let level = self.resources["level"];
                let low = c
                    .levels
                    .iter()
                    .find(|l| l.id == level)
                    .map_or(0, |l| l.experience);
                let high = c
                    .levels
                    .iter()
                    .find(|l| l.id == level + 1)
                    .map_or(low, |l| l.experience);
                (low, high)
            })
            .unwrap_or((0, 0));
        v.progress.insert(
            "exp_progress".into(),
            if max == min {
                1.0
            } else {
                ((self.resources["exp"] - min) as f64 / (max - min) as f64).clamp(0.0, 1.0)
            },
        );
        // CHUDLayer.updateGUI leaves its authored full bar unchanged when
        // the rank has no next threshold (including the initial no-league rank).
        v.progress.insert("honor_progress".into(), 1.0);
        v
    }
}

#[cfg(test)]
mod density_checks {
    use super::*;

    #[test]
    fn retina_keeps_controls_the_same_size_in_css_pixels() {
        for (width, height) in [(844, 390), (1920, 1080), (390, 844)] {
            let css_scale = scale_at_density(
                &Camera {
                    x: 0.,
                    y: 0.,
                    zoom: 1.,
                    width,
                    height,
                },
                1.,
            );
            for density in [1.5, 2., 3.] {
                let camera = Camera {
                    x: 0.,
                    y: 0.,
                    zoom: 1.,
                    width: (width as f64 * density) as u32,
                    height: (height as f64 * density) as u32,
                };
                assert!((scale_at_density(&camera, density) / density - css_scale).abs() < 1e-9);
            }
        }
    }
}
