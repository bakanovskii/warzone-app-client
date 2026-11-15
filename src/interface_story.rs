//! Original PersonageDialog portraits and three-piece expanding speech bubble.
use crate::{
    Camera, Scene,
    interface::{self, Action, Hit, View},
    production::State,
};

/// Transient PersonageDialog animation; never serialized into the profile.
/// Source delays0.05/0.06, portrait/balloon0.1, text0.5, then unlock/1s tap
/// pulse. onButtonOK updates later phrases with useAnimation:NO.
#[derive(Default)]
pub struct Playback {
    group: Option<String>,
    key: String,
    opened: f64,
    phrase_at: f64,
    animated: bool,
}
pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    let mut lo: f64 = 0.;
    let mut hi: f64 = 1.;
    // UIView's default ease-in/ease-out timing curve(.42,0,.58,1).
    for _ in 0..20 {
        let p = (lo + hi) * 0.5;
        let x = 3. * (1. - p).powi(2) * p * 0.42 + 3. * (1. - p) * p * p * 0.58 + p * p * p;
        if x < t { lo = p } else { hi = p }
    }
    let p = (lo + hi) * 0.5;
    3. * (1. - p) * p * p + p * p * p
}
impl Playback {
    pub fn observe(&mut self, scene: &Scene, state: &State, visible: bool, seconds: f64) {
        let Some((key, _)) = crate::story::current(scene, state).filter(|_| visible) else {
            self.group = None;
            self.key.clear();
            return;
        };
        let (group, animated) = if let Some(p) = state.quest_story().ok().flatten() {
            (format!("quest:{}", p.quest), true)
        } else if let Some(m) = &state.mission {
            if let Some(message) = m.battle.story.as_ref().and_then(|s| s.message.as_ref()) {
                (format!("{}:{}", m.id, message.mask), true)
            } else if key.starts_with("#pvptutorial_1_1star_") {
                (format!("{}:firststar", m.id), true)
            } else {
                (format!("{}:{key}", m.id), false)
            }
        } else {
            (key.clone(), false)
        };
        if self.group.as_ref() != Some(&group) {
            self.group = Some(group);
            self.opened = seconds;
            self.phrase_at = seconds;
            self.animated = animated;
        } else if self.key != key {
            self.animated = false;
            self.phrase_at = seconds;
        }
        self.key = key;
    }
    pub fn input_ready(&self, seconds: f64) -> bool {
        if self.group.is_none() {
            return true;
        }
        if self.key == "#pvptutorial_1_1star_1_alice_left" && seconds - self.phrase_at < 2.5 {
            return false;
        }
        !self.animated || seconds - self.opened >= 0.66
    }
    pub fn snapshot(&self, seconds: f64) -> serde_json::Value {
        serde_json::json!({"key":self.key,"animated_entry":self.animated,"elapsed":(seconds-self.opened).max(0.),"input_ready":self.input_ready(seconds)})
    }
    pub fn apply(&self, v: &mut View, camera: &Camera, seconds: f64) {
        if self.group.is_none() || self.key == "#pvp_end_game_short_info" {
            return;
        }
        let elapsed = (seconds - self.opened).max(0.);
        let scale = crate::hud::scale(camera);
        let slide = if self.animated {
            ease((elapsed - 0.05) / 0.1)
        } else {
            1.
        };
        let balloon = if self.animated {
            ease((elapsed - 0.06) / 0.1)
        } else {
            1.
        };
        let text = if self.animated {
            ease((elapsed - 0.16) / 0.5)
        } else {
            1.
        };
        let tap_since = if self.animated {
            (elapsed - 0.66).max(0.)
        } else {
            (seconds - self.phrase_at).max(0.)
        };
        let phase = tap_since % 2.;
        let pulse = if phase < 1. {
            1. - 0.6 * ease(phase)
        } else {
            0.4 + 0.6 * ease(phase - 1.)
        };
        let right = crate::story::character(&self.key).2;
        for draw in &mut v.draws {
            if draw
                .node
                .parents
                .iter()
                .any(|p| p == "source_story_portrait")
            {
                let end = draw.node.rect[0] + draw.node.rect[2] / 2.;
                let start = if right {
                    camera.width as f64 + 80. * scale
                } else {
                    -80. * scale
                };
                draw.node.rect[0] += (start - end) * (1. - slide);
            }
            if draw
                .node
                .parents
                .iter()
                .any(|p| p == "source_story_balloon")
            {
                let opacity = if draw.node.id == "hint_txt" {
                    balloon * text
                } else if draw.node.id == "tap_txt" {
                    if self.animated && elapsed < 0.66 {
                        0.
                    } else {
                        pulse
                    }
                } else {
                    balloon
                };
                draw.opacity = (draw.opacity as f64 * opacity).round() as u8;
            }
        }
        if !self.input_ready(seconds) {
            v.hits.clear();
        }
    }
}
pub fn add(v: &mut View, scene: &Scene, state: &State, camera: &Camera) -> Option<()> {
    let (key, text) = crate::story::current(scene, state)?;
    add_message(v, scene, state, camera, &key, &text)
}
pub fn add_message(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    key: &str,
    text: &str,
) -> Option<()> {
    if key == "#pvp_end_game_short_info" {
        crate::interface_windows::message(v, scene, camera, text, false);
        return Some(());
    }
    let c = scene.interface.as_ref()?;
    let tutorial = state
        .mission
        .as_ref()
        .and_then(|s| s.battle.tutorial_step(scene.combat.as_ref()?));
    let (index, name, right) = if tutorial.is_some() {
        (4, "alisa", false)
    } else {
        crate::story::character(key)
    };
    let portrait = c.symbols.get(&format!("story_{name}"))?;
    let balloon = c.symbols.get("story_balloon")?;
    let scale = crate::hud::scale(camera);
    let width = camera.width as f64 / scale;
    let height = camera.height as f64 / scale;
    let origin = [243. - 521. / 2., 155. - 336. / 2.];
    let (head, flip, shift) = crate::story::offsets(index, right);
    let action_hint = tutorial.is_some_and(|s| s.button.is_empty());
    if !action_hint {
        v.hits.clear();
        v.blocks.clear();
        v.modal = true;
    }
    if !action_hint {
        v.popup_shade(balloon.nodes.first()?, camera);
        v.draws.last_mut()?.opacity = 51; // PersonageDialog UIView background alpha.2
    }
    let f = portrait.slots["initial_frame"];
    let original_center = f[0] + f[2] / 2.;
    let center = if (5..=7).contains(&index) {
        if right {
            original_center + (width - 568.).max(0.)
        } else {
            width - original_center
        }
    } else if right {
        width - original_center + if !(5..=7).contains(&index) { flip } else { 0. }
    } else {
        original_center
    };
    // Source init anchors full portraits to screen bottom+15 in their parent.
    let rect = [
        origin[0] + center - f[2] / 2.,
        origin[1] + height - f[3] + if index == 4 { -22. } else { 15. },
        f[2],
        f[3],
    ];
    let start = v.draws.len();
    v.add(portrait, [0., 0.], scale, |_| true, |_| None, None);
    for d in &mut v.draws[start..] {
        d.node.rect = [
            rect[0] * scale,
            rect[1] * scale,
            rect[2] * scale,
            rect[3] * scale,
        ];
        d.node.flip_x = if (5..=7).contains(&index) {
            !right
        } else {
            right
        };
        d.node.parents.push("source_story_portrait".into());
    }
    let b = balloon.slots["initial_frame"];
    let start = v.draws.len();
    v.add(
        balloon,
        [
            origin[0] + b[0] + shift + if right { (width - 568.).max(0.) } else { 0. },
            origin[1] + b[1],
        ],
        scale,
        |n| {
            !(action_hint && n.id == "tap_txt")
                && !n.parents.iter().any(|p| p == "real_money")
                && n.id != if right { "arrowLeft" } else { "arrowRight" }
        },
        |n| match n.id.as_str() {
            "hint_txt" => Some(text.to_owned()),
            "tap_txt" => Some(interface::lang(scene, "#tap_to_continue", "").into()),
            _ => None,
        },
        None,
    );
    for d in &mut v.draws[start..] {
        // Render reflows this group with the same font measurement as the text
        // rasterizer; source header position is screenHeight-270+headShift.
        d.node.parents.push("source_story_balloon".into());
        d.node.parents.push(format!("story_head_{}", head as i32));
        d.node
            .parents
            .push(format!("story_height_{}", camera.height));
        if d.node.id == "arrowRight" {
            d.node.parents.push("story_right".into());
        }
    }
    if !action_hint {
        v.hits.push(Hit {
            action: Action::Confirm,
            rect: [0., 0., camera.width as f64, camera.height as f64],
        });
    }
    Some(())
}
pub fn focus(v: &mut View, scene: &Scene, state: &State, camera: &Camera) -> Option<()> {
    focus_animated(v, scene, state, camera, 0.)
}
pub fn focus_animated(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    seconds: f64,
) -> Option<()> {
    let c = scene.interface.as_ref()?;
    let balloon = c.symbols.get("story_balloon")?;
    let scale = crate::hud::scale(camera);
    let step = state
        .mission
        .as_ref()?
        .battle
        .tutorial_step(scene.combat.as_ref()?)?
        .clone();
    if !step.hole {
        return Some(());
    }
    let mut point = step.point();
    if !step.unit.is_empty() {
        if let Some(u) = state
            .mission
            .as_ref()?
            .battle
            .units
            .iter()
            .find(|u| u.uid == step.unit)
        {
            point = [u.x as i32 - 16, u.y as i32 - 16];
        }
    }
    let mut center = if step.screen {
        let action = if step.id == 9 {
            Action::Close
        } else {
            Action::Primary
        };
        let r = v.hits.iter().find(|h| h.action == action)?.rect;
        [r[0] + r[2] / 2., r[1] + r[3] / 2.]
    } else {
        camera.screen(
            scene
                .projection
                .project(point[0] as f64 + 0.5, point[1] as f64 + 0.5),
        )
    };
    if [5, 101, 7, 62].contains(&step.id) {
        center[1] -= 8. * scale;
    } else if [6, 61].contains(&step.id) {
        center[1] -= 4. * scale;
    }
    let size = if [2, 3].contains(&step.id) {
        [100. * scale, 50. * scale]
    } else if step.id == 9 {
        [250. * scale, 75. * scale]
    } else if step.id == 4 {
        let r = v.hits.iter().find(|h| h.action == Action::Primary)?.rect;
        [r[2], 55. * scale]
    } else {
        [100. * scale, 80. * scale]
    };
    let template = balloon.nodes.first()?;
    let mut overlay = View::default();
    // showHole... creates UIViewWithHole at alpha .25 even when bUseDark is
    // false. The latter controls the separate non-hole lockGray path only.
    crate::quest_navigation::draw_source_focus(
        &mut overlay,
        template,
        [
            center[0] - size[0] / 2.,
            center[1] - size[1] / 2.,
            size[0],
            size[1],
        ],
        c.symbols.get("tutorial_ring"),
    );
    // Tutorials.showHole... creates a second hole at(45,H-38), with the
    // same width and ten extra points of height, not the troop hitbox size.
    let card = if [2, 3].contains(&step.id) {
        Some([
            -5. * scale,
            camera.height as f64 - 68. * scale,
            100. * scale,
            60. * scale,
        ])
    } else {
        None
    };
    if let Some(r) = card {
        v.extra_tutorial_holes.push(r);
        let mut focus = View::default();
        crate::quest_navigation::draw_source_focus(
            &mut focus,
            template,
            r,
            c.symbols.get("tutorial_ring"),
        );
        overlay.draws.extend(
            focus
                .draws
                .into_iter()
                .filter(|d| d.node.kind != "tutorial_mask"),
        );
    }
    // Both the focus outline and its shade belong below PersonageDialog.
    let at = v
        .draws
        .iter()
        .position(|d| d.node.parents.iter().any(|p| p == "source_story_portrait"))
        .unwrap_or(v.draws.len());
    v.draws.splice(at..at, overlay.draws);
    if step.arrow {
        if let Some(arrow) = c.symbols.get("tutorial_arrow") {
            let (phase, squash) = crate::quest_navigation::tap_pose(seconds);
            let a = arrow.nodes.first()?.rect;
            let position = if card.is_some() {
                let start = [45. * scale, camera.height as f64 - 90. * scale];
                let mut end =
                    camera.screen(scene.projection.project(point[0] as f64, point[1] as f64));
                end[1] -= 30. * scale;
                // showArrowWithAnimation uses2s for the diagonal troop gesture.
                let t = ease((seconds.max(0.) % 2.) / 2.);
                [
                    start[0] + (end[0] - start[0]) * t,
                    start[1] + (end[1] - start[1]) * t,
                ]
            } else if step.id == 4 {
                let r = v.hits.iter().find(|h| h.action == Action::Primary)?.rect;
                [
                    center[0],
                    r[1] - a[3] * scale / 2. - 15. * scale * (1. - phase),
                ]
            } else {
                let p = camera.screen(
                    scene
                        .projection
                        .project(point[0] as f64 - 1., point[1] as f64 - 1.),
                );
                [p[0], p[1] + 10. * scale + 15. * scale * phase]
            };
            let start = v.draws.len();
            v.add(
                arrow,
                [
                    position[0] / scale - a[0] - a[2] / 2.,
                    position[1] / scale - a[1] - a[3] / 2.,
                ],
                scale,
                |_| true,
                |_| None,
                None,
            );
            if card.is_none() {
                for d in &mut v.draws[start..] {
                    let bottom = d.node.rect[1] + d.node.rect[3];
                    d.node.rect[3] *= 1. - 0.25 * squash;
                    d.node.rect[1] = bottom - d.node.rect[3];
                }
            }
        }
    }
    Some(())
}
