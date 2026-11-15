//! Original league selector and horizontally paged rules. Server rows stay absent offline.
use crate::{
    Camera, Scene,
    interface::{self, Action, View},
    production::State,
};

pub const LEAGUES: [&str; 4] = ["wooden", "bronze", "silver", "gold"];

pub fn current(state: &State) -> usize {
    let name = state
        .systems
        .as_ref()
        .and_then(|s| s.preferences.get("league"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    LEAGUES.iter().position(|n| *n == name).unwrap_or(4)
}
pub fn button(v: &mut View, scene: &Scene, camera: &Camera) -> Option<()> {
    let root = scene.interface.as_ref()?.symbols.get("leagues_button")?;
    let (o, scale) = interface::sidebar_origin(camera, root.size, 2, false);
    v.add(root, o, scale, |_| true, |_| None, None);
    v.button(root, "leaderboard", o, scale, Action::PvpLeagues);
    Some(())
}
pub fn build(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    selected: usize,
) -> Option<()> {
    let responsive = interface::wide_panel(
        scene.interface.as_ref()?.symbols.get("leagues")?,
        camera,
        &["close_btn", "help", "help_btn"],
    );
    let root = &responsive;
    let scale = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / scale - root.size[0]) / 2.,
        camera.height as f64 / scale - root.size[1],
    ];
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.add(
        root,
        o,
        scale,
        |n| {
            !LEAGUES.iter().any(|name| {
                n.id == format!("{name}_choosen") && LEAGUES.get(selected).copied() != Some(*name)
            })
        },
        |n| {
            let key = match n.id.as_str() {
                "No_word" => "#league_wooden",
                "bronze_word" => "#league_bronze",
                "Silver" => "#league_silver",
                "Gold_word" => "#league_gold",
                "league_no" => "#league_type_no",
                "league_bronze" | "league_silver" | "league_gold" => "#league_type",
                _ => "",
            };
            Some(if let Some(name) = n.id.strip_prefix("Honor_txt_") {
                state
                    .systems
                    .as_ref()
                    .map_or(0, |s| s.integer(&format!("{name}Limit")))
                    .to_string()
            } else {
                if key.is_empty() {
                    n.text.clone()
                } else {
                    interface::lang(scene, key, "").into()
                }
            })
        },
        None,
    );
    // Match the slightly tracked Russian uppercase titles in the supplied
    // same-size iPhone comparison; leave secondary league labels unchanged.
    for d in &mut v.draws {
        if ["No_word", "bronze_word", "Silver", "Gold_word"].contains(&d.node.id.as_str()) {
            d.node.letter_spacing = 0.3 * scale;
        }
    }
    for (i, name) in LEAGUES.iter().enumerate() {
        v.button(
            root,
            &format!("{name}_choosen_btn"),
            o,
            scale,
            Action::LeagueSelect(i),
        );
    }
    v.button(root, "close_btn", o, scale, Action::Close);
    v.button(root, "help_btn", o, scale, Action::LeaguesHelp);
    Some(())
}
pub fn clip(scene: &Scene, camera: &Camera) -> Option<[f64; 4]> {
    let responsive = help_panel(scene, camera)?;
    let root = &responsive;
    let s = crate::hud::scale(camera);
    let r = root.frames.get("picture_frame")?;
    Some([
        ((camera.width as f64 / s - root.size[0]) / 2. + r[0]) * s,
        (camera.height as f64 / s - root.size[1] + r[1]) * s,
        r[2] * s,
        r[3] * s,
    ])
}
fn help_panel(scene: &Scene, camera: &Camera) -> Option<interface::Symbol> {
    let source = scene.interface.as_ref()?.symbols.get("leagues_help")?;
    let mut panel = interface::wide_panel(source, camera, &["close_btn"]);
    let extra = panel.size[0] - source.size[0];
    // Only the outer shade spans the window. The authored picture backing and
    // pages stay centred at their original size, alongside their clipping frame.
    for (node, original) in panel.nodes.iter_mut().zip(&source.nodes) {
        if original.kind == "image"
            && original.parents.is_empty()
            && original.rect[2] >= 450.
            && original.rect[2] < source.size[0] - 1.
        {
            node.rect = original.rect;
            node.rect[0] += extra / 2.;
        }
    }
    Some(panel)
}
pub fn page_width(scene: &Scene, camera: &Camera) -> Option<f64> {
    Some(
        scene
            .interface
            .as_ref()?
            .symbols
            .get("leagues_help")?
            .frames
            .get("picture_frame")?[2]
            * crate::hud::scale(camera),
    )
}
pub fn page(scene: &Scene, camera: &Camera, scroll: f64) -> usize {
    page_width(scene, camera).map_or(0, |w| (scroll / w).round().clamp(0., 4.) as usize)
}
pub fn help(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    scroll: f64,
) -> Option<()> {
    let c = scene.interface.as_ref()?;
    let responsive = help_panel(scene, camera)?;
    let root = &responsive;
    let scale = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / scale - root.size[0]) / 2.,
        camera.height as f64 / scale - root.size[1],
    ];
    let frame = root.frames.get("picture_frame")?;
    let clip = [
        (o[0] + frame[0]) * scale,
        (o[1] + frame[1]) * scale,
        frame[2] * scale,
        frame[3] * scale,
    ];
    let selected = page(scene, camera, scroll);
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.scroll_max = frame[2] * 4. * scale;
    v.add(
        root,
        o,
        scale,
        |n| {
            n.id.strip_prefix("choose")
                .and_then(|s| s.parse::<usize>().ok())
                .is_none_or(|i| i == selected + 1)
        },
        |n| {
            Some(if n.id == "Header" {
                interface::lang(scene, "#leaguerules_title", "").into()
            } else {
                String::new()
            })
        },
        None,
    );
    for i in 0..5 {
        // A swipe needs only the page(s) intersecting this frame. Don't clone
        // and serialize all five sheets on every pointer motion and frame.
        let left = i as f64 * frame[2] * scale - scroll;
        if left >= clip[2] || left + frame[2] * scale <= 0. {
            continue;
        }
        let row = c.symbols.get(&format!("leagues_help_img{}", i + 1))?;
        let pos = [
            o[0] + frame[0] + i as f64 * frame[2] + (frame[2] - row.size[0]) / 2. - scroll / scale,
            o[1] + frame[1] + (frame[3] - row.size[1]) / 2.,
        ];
        v.add(
            row,
            pos,
            scale,
            |_| true,
            |n| {
                let value = |name: &str| {
                    state
                        .systems
                        .as_ref()
                        .and_then(|s| s.preferences.get(name))
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .unwrap_or_default()
                };
                Some(if let Some(name) = n.id.strip_suffix("_honor") {
                    state
                        .systems
                        .as_ref()
                        .map_or(0, |s| s.integer(&format!("{name}Limit")))
                        .to_string()
                } else if let Some(name) = n.id.strip_prefix("cash_") {
                    value(&format!("{name}Reward"))
                } else if n.id == format!("text{}", i + 1) {
                    let mut text =
                        interface::lang(scene, &format!("#leaguerules_page_{}", i + 1), "")
                            .to_string();
                    for (j, name) in ["bronze", "silver", "gold"].iter().enumerate() {
                        text =
                            text.replace(&format!("[{}]", j + 1), &value(&format!("{name}Reward")));
                    }
                    text
                } else {
                    n.text.clone()
                })
            },
            Some(clip),
        );
    }
    v.button(root, "close_btn", o, scale, Action::Close);
    Some(())
}
