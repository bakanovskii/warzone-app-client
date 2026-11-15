//! AchievementsDialog source geometry, clipped vertical list and real awards.
use crate::{
    Camera, Scene,
    interface::{self, Action, View},
    production::State,
};
pub fn button(v: &mut View, scene: &Scene, state: &State, camera: &Camera) -> Option<()> {
    let root = scene
        .interface
        .as_ref()?
        .symbols
        .get("achievements_button")?;
    let (o, scale) = interface::sidebar_origin(camera, root.size, 1, false);
    let count = state.achievement_awards_pending(scene);
    v.add(
        root,
        o,
        scale,
        |n| count > 0 || !n.parents.iter().any(|p| p == "bcm_achiev_count"),
        |n| (n.id == "achiev_count_txt").then(|| count.to_string()),
        None,
    );
    v.button(root, "achievements", o, scale, Action::Achievements);
    Some(())
}
pub fn build(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    scroll: f64,
) -> Option<()> {
    let c = scene.interface.as_ref()?;
    let responsive = interface::wide_panel(c.symbols.get("achievements")?, camera, &["close_btn"]);
    let root = &responsive;
    let row = c.symbols.get("achievement_row")?;
    let defs = &scene.achievements.as_ref()?.definitions;
    let scale = crate::hud::scale(camera);
    let w = camera.width as f64 / scale;
    let h = camera.height as f64 / scale;
    // AchievementsDialog.moveUp: phone y=screenHeight-viewHeight+84.
    let o = [(w - root.size[0]) / 2., h - root.size[1] + 84.];
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    crate::interface_windows::quest_badges(v, scene, state, camera, false);
    let badge_end = v.draws.len();
    // This source window shades through its own background PNG, not a global popup overlay.
    let text = |n: &interface::Node| {
        Some(match n.id.as_str() {
            "title_window" => interface::lang(scene, "#achievements_title", "").into(),
            "gc_hint" => interface::lang(scene, "#achievements_also_in_game_center", "").into(),
            "gcBtn_txt" => interface::lang(scene, "#button_challenge_friends", "").into(),
            _ => String::new(),
        })
    };
    v.add(root, o, scale, |_| true, text, None);
    // AchievementsDialog.updateGUI0x119xxx changes the Russian phone caption
    // from14 to11 points, then centers it in its source button.
    if state.language_id() == 1 {
        for d in &mut v.draws {
            if d.node.id == "gcBtn_txt" {
                d.node.font_size = 11. * scale;
                d.node.line_height = 13.2 * scale;
                // MTLabel's authored110pt frame is too narrow for the localized
                // caption even after the Russian11pt override. Source centers the
                // complete line; retain its centre while giving the line its ink.
                let width = 140. * scale;
                d.node.rect[0] += (d.node.rect[2] - width) * 0.5;
                d.node.rect[2] = width;
                d.node.rect[1] = (o[1] + 244.275 + (30. - 18.) * 0.5) * scale;
            }
        }
    }
    let front = v
        .draws
        .iter()
        .filter(|d| d.node.parents.iter().any(|p| p == "woman_img"))
        .map(|d| d.node.clone())
        .collect::<Vec<_>>();
    v.draws
        .retain(|d| !d.node.parents.iter().any(|p| p == "woman_img"));
    // Phone scroll frame460+10 wide,200 high; rows start12, int(height)+5.
    let offset = scroll / scale;
    let clip = [
        (o[0] + 69. + (w - 568.).max(0.) / 2.) * scale,
        (o[1] + 40.) * scale,
        470. * scale,
        200. * scale,
    ];
    let first = 12.;
    let stride = row.size[1].trunc() + 5.;
    v.scroll_max = ((defs.len() as f64 * stride + 25.) - 200.).max(0.) * scale;
    for (index, d) in defs.iter().enumerate() {
        let p = state.achievement_progress(d);
        let origin = [
            o[0] + 69. + (w - 568.).max(0.) / 2.,
            o[1] + 40. + first + index as f64 * stride - offset,
        ];
        let start = v.draws.len();
        v.add(
            row,
            origin,
            scale,
            |n| {
                if let Some(i) =
                    n.id.strip_prefix("achiev_star")
                        .and_then(|s| s.parse::<usize>().ok())
                {
                    return i <= p.stars;
                }
                if n.id == "achiev_complete" {
                    return p.tier > 3;
                }
                if n.parents.iter().any(|s| s == "achiev_award_group") {
                    if p.tier > 3 {
                        return false;
                    }
                    if n.parents.iter().any(|s| s == "achiev_progressbar") {
                        return !p.ready;
                    }
                    if n.parents.iter().any(|s| s == "awardBtn") {
                        return p.ready;
                    }
                }
                true
            },
            |n| {
                Some(match n.id.as_str() {
                    "achiev_title" => {
                        interface::lang(scene, &format!("#ACHIEVEMENT_{}_NAME", d.id), "").into()
                    }
                    "achiev_txt" => interface::lang(
                        scene,
                        &format!("#ACHIEVEMENT_{}_{}_TEXT", d.id, p.tier.min(3)),
                        "",
                    )
                    .into(),
                    "progress_txt" => format!("{}/{}", p.count.min(p.need), p.need),
                    "award_txt" => interface::lang(scene, "#achievements_label_reward", "").into(),
                    "award_money" => p.award.to_string(),
                    "awardBtn_txt" => interface::lang(scene, "#button_take_reward", "").into(),
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        for draw in &mut v.draws[start..] {
            if draw.node.id == "progress" {
                draw.progress = (p.count as f32 / p.need as f32).clamp(0., 1.) as f64;
            }
        }
        if p.ready {
            v.button(
                row,
                "awardBtn",
                origin,
                scale,
                Action::ClaimAchievement(d.id, p.tier),
            );
            let hit = v.hits.last_mut()?;
            let r = hit.rect;
            let x = r[0].max(clip[0]);
            let y = r[1].max(clip[1]);
            let right = (r[0] + r[2]).min(clip[0] + clip[2]);
            let bottom = (r[1] + r[3]).min(clip[1] + clip[3]);
            if right > x && bottom > y {
                hit.rect = [x, y, right - x, bottom - y];
            } else {
                v.hits.pop();
            }
        }
    }
    // The source portrait sits above scroll content and may overlap its last row.
    for node in front {
        v.draws.push(interface::Draw {
            node,
            clip: None,
            progress: 1.,
            opacity: 255,
            grayscale: false,
        });
    }
    for draw in &mut v.draws[badge_end..] {
        draw.node.parents.push("source_bottom_slide".into());
    }
    v.button(root, "close_btn", o, scale, Action::Close);
    v.button(root, "gcBtn", o, scale, Action::GameCenter);
    Some(())
}
