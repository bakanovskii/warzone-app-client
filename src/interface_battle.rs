//! Original phone deployment frame, troop cards and mission-goals dialog.
//! CHUDBattle.createSoldiersListWnd/updateSoldiersList and MissionGoalsDialog.
use crate::{
    Camera, Scene,
    interface::{self, Action, Draw, Hit, View},
    production::State,
};

pub fn available(scene: &Scene) -> bool {
    scene.interface.as_ref().is_some_and(|c| {
        [
            "deployment",
            "deployment_card",
            "battle_goals",
            "battle_goals_button",
            "battle_hud",
        ]
        .iter()
        .all(|k| c.symbols.contains_key(*k))
    })
}
pub fn types(_scene: &Scene, state: &State) -> Vec<u32> {
    state
        .mission
        .as_ref()
        .map_or_else(Vec::new, |m| crate::battle_selection::types(&m.battle))
}
pub fn bottom_top(scene: &Scene, camera: &Camera) -> f64 {
    let s = crate::hud::scale(camera);
    let h = scene
        .interface
        .as_ref()
        .and_then(|c| c.symbols.get("deployment"))
        .map_or(96.075, |s| s.size[1]);
    camera.height as f64 - (h - 13.) * s
}
fn thumbnail(
    v: &mut View,
    scene: &Scene,
    state: &State,
    id: u32,
    rect: [f64; 4],
    clip: Option<[f64; 4]>,
    opacity: u8,
    drag: bool,
) {
    let Some(texture) = state.unit_thumbnail(scene, id) else {
        return;
    };
    let c = scene.interface.as_ref().unwrap();
    let meta = &scene.textures[texture];
    let ratio = (rect[2] / meta.width as f64).min(rect[3] / meta.height as f64);
    let (w, h) = (meta.width as f64 * ratio, meta.height as f64 * ratio);
    let mut node = c.symbols["deployment_card"]
        .nodes
        .iter()
        .find(|n| n.kind == "image")
        .unwrap()
        .clone();
    node.texture = texture.into();
    node.id = if drag {
        "deployment_drag".into()
    } else {
        format!("deployment_unit_{id}")
    };
    node.parents = vec![format!("deployment_card_{id}")];
    node.rect = [
        rect[0] + (rect[2] - w) * 0.5,
        rect[1] + (rect[3] - h) * 0.5,
        w,
        h,
    ];
    v.draws.push(Draw {
        node,
        clip,
        progress: 1.,
        opacity,
        grayscale: false,
    });
}
/// The historical drop point is below the finger, not the thumbnail center.
/// CHUDBattle.onButtonSoldierDragFinish uses (+2,+25) logical points.
pub fn drop_cell(scene: &Scene, camera: &Camera, point: [i32; 2]) -> [i32; 2] {
    let s = crate::hud::scale(camera);
    let w = camera.world([point[0] as f64 + 2. * s, point[1] as f64 + 25. * s]);
    let p = scene.projection.inverse(w[0], w[1]);
    [(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32]
}
pub fn return_thumbnail(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    id: u32,
    p: [i32; 2],
) {
    let Some(texture) = state.unit_thumbnail(scene, id) else {
        return;
    };
    let meta = &scene.textures[texture];
    let s = crate::hud::scale(camera);
    thumbnail(
        v,
        scene,
        state,
        id,
        [
            p[0] as f64 - meta.width as f64 * s / 4.,
            p[1] as f64 - meta.height as f64 * s / 4.,
            meta.width as f64 * s / 2.,
            meta.height as f64 * s / 2.,
        ],
        None,
        255,
        true,
    );
}
pub fn build(
    scene: &Scene,
    state: &State,
    camera: &Camera,
    scroll: f64,
    drag: Option<(u32, [i32; 2])>,
) -> Option<View> {
    if !available(scene) {
        return None;
    };
    let session = state.mission.as_ref()?;
    let b = &session.battle;
    let c = scene.interface.as_ref()?;
    let s = crate::hud::scale(camera);
    let mut v = View::default();
    if b.pvp.is_some() {
        crate::interface_pvp::arena(&mut v, scene, state, camera);
    }
    if b.pvp_intro_active(scene.combat.as_ref()?) {
        return Some(v);
    }
    // Tutorials.showTutorial first reveals deployment at ID100 (index4).
    // Its introductory portrait owns the screen before that; ordinary mission
    // goals / pause affordances are unavailable during the scripted lesson.
    if b.tutorial.as_ref().is_some_and(|t| t.index < 4) {
        return Some(v);
    }
    let flag = &c.symbols["battle_goals_button"];
    if b.tutorial.is_none() && b.pvp.is_none() {
        v.add(flag, [33., 85.], s, |_| true, |_| None, None);
    }
    if b.tutorial.is_none() && b.pvp.is_none() {
        v.button(flag, "goals_button", [33., 85.], s, Action::Quest(0));
    }
    if b.phase != "placement" || b.result.is_some() {
        let finish_button = b.result.is_some() || b.pvp_score().is_some_and(|s| s.stars > 0);
        let root = &c.symbols["battle_hud"];
        let o = [
            camera.width as f64 / s - root.size[0] - 5.,
            camera.height as f64 / s - root.size[1],
        ];
        v.add(
            root,
            o,
            s,
            |n| {
                !n.parents.iter().any(|p| {
                    if finish_button {
                        p == "defeat_btn"
                    } else {
                        p == "endbattle_btn"
                    }
                })
            },
            |_| Some(String::new()),
            None,
        );
        v.button(root, "skipmove_btn", o, s, Action::Primary);
        v.button(root, "tobase_btn", o, s, Action::Close);
        v.button(
            root,
            if finish_button {
                "endbattle_btn"
            } else {
                "defeat_btn"
            },
            o,
            s,
            Action::Cancel,
        );
        v.blocks
            .push([o[0] * s, o[1] * s, root.size[0] * s, root.size[1] * s]);
        return Some(v);
    };
    let responsive = interface::wide_panel(
        &c.symbols["deployment"],
        camera,
        &["battlestart_btn", "battlestart_txt", "battlereturn_btn"],
    );
    let root = &responsive;
    let o = [
        camera.width as f64 / s - root.size[0],
        camera.height as f64 / s - root.size[1],
    ];
    v.add(
        root,
        o,
        s,
        |n| !n.parents.iter().any(|p| p == "dragHintArea"),
        |n| {
            Some(match n.id.as_str() {
                "battlestart_txt" => {
                    interface::lang(scene, "#battle_place_ready_btn", "В БОЙ!").into()
                }
                "battlereturn_txt" => interface::lang(scene, "#battle_place_home_btn", "").into(),
                _ => String::new(),
            })
        },
        None,
    );
    v.button(root, "battlestart_btn", o, s, Action::Primary);
    v.button(root, "battlereturn_btn", o, s, Action::Close);
    v.blocks.push([
        0.,
        bottom_top(scene, camera),
        camera.width as f64,
        camera.height as f64 - bottom_top(scene, camera),
    ]);
    let ids = types(scene, state);
    // Source reserves130points at right for Start. Phone480 starts at130;
    // widescreen phone starts at45. Step74, centerY=int(height-40+6).
    let first = if camera.width as f64 / s > 480.001 {
        45.
    } else {
        130.
    };
    let width = root.size[0] - 130.;
    v.scroll_max = ((first + ids.len() as f64 * 74. - width).max(0.)) * s;
    let shift = scroll / s;
    let clip = [o[0] * s, o[1] * s, width * s, root.size[1] * s];
    let template = &c.symbols["deployment_card"];
    for (index, id) in ids.iter().enumerate() {
        let count = b.available(*id);
        let center = [
            first + index as f64 * 74. - shift,
            (root.size[1] - 34.).trunc(),
        ];
        let origin = [
            o[0] + center[0] - template.size[0] * 0.5,
            o[1] + center[1] - template.size[1] * 0.5 - 5.,
        ];
        let name = interface::lang(scene, &format!("#item_{id}_name"), "");
        let mut t = template.clone();
        if name.encode_utf16().count() > 10 {
            for n in &mut t.nodes {
                if n.id == "unit_name" {
                    n.rect[1] -= 2.;
                    n.rect[3] += 10.;
                }
            }
        }
        v.add(
            &t,
            origin,
            s,
            |_| true,
            |n| {
                Some(match n.id.as_str() {
                    "unit_name" => name.into(),
                    "unit_counter" => count.to_string(),
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        let r = t.frames["thumbnail_place"];
        let rect = [
            (origin[0] + r[0]) * s,
            (origin[1] + r[1]) * s,
            r[2] * s,
            r[3] * s,
        ];
        thumbnail(
            &mut v,
            scene,
            state,
            *id,
            rect,
            Some(clip),
            if count > 0 && !drag.is_some_and(|(kind, _)| kind == *id) {
                255
            } else {
                102
            },
            false,
        );
        if count > 0 {
            let x = rect[0].max(clip[0]).max(0.);
            let y = rect[1].max(clip[1]).max(0.);
            let right = (rect[0] + rect[2])
                .min(clip[0] + clip[2])
                .min(camera.width as f64);
            let bottom = (rect[1] + rect[3])
                .min(clip[1] + clip[3])
                .min(camera.height as f64);
            if right > x && bottom > y {
                v.hits.push(Hit {
                    action: Action::Choose(index),
                    rect: [x, y, right - x, bottom - y],
                });
            }
        }
    }
    if let Some((id, p)) = drag {
        // onButtonSoldierAdd loads the natural phone UIImage size; the
        // (+2,+25) offset belongs only to the placement tile, not the image.
        let texture = state.unit_thumbnail(scene, id)?;
        let meta = &scene.textures[texture];
        let r = [
            p[0] as f64 - meta.width as f64 * s / 4.,
            p[1] as f64 - meta.height as f64 * s / 4.,
            meta.width as f64 * s / 2.,
            meta.height as f64 * s / 2.,
        ];
        if let Some(art) = scene.combat.as_ref().and_then(|c| c.selection.as_ref()) {
            let cell = drop_cell(scene, camera, p);
            let b = &state.mission.as_ref()?.battle;
            let allowed = b.placement_cell(scene.combat.as_ref()?, cell, id);
            let valid = allowed.is_some();
            let preview = allowed.unwrap_or(cell);
            let begin = v.draws.len();
            crate::battle_selection::sprite(
                &mut v,
                scene,
                camera,
                &art.tiles["move"],
                scene
                    .projection
                    .project(preview[0] as f64, preview[1] as f64),
                0.,
                128,
                true,
            );
            for d in &mut v.draws[begin..] {
                d.node.kind = "deployment_tile".into();
                d.node.color = if valid { 0x00ff00 } else { 0xff0000 };
            }
        }
        thumbnail(&mut v, scene, state, id, r, None, 255, true);
    }
    Some(v)
}
pub fn goals(v: &mut View, scene: &Scene, state: &State, camera: &Camera) {
    let Some(session) = &state.mission else {
        return;
    };
    let Some(c) = &scene.interface else { return };
    let Some(root) = c.symbols.get("battle_goals") else {
        return;
    };
    let m = &scene.combat.as_ref().unwrap().missions[&session.battle.mission];
    let objectives: Vec<_> = session
        .battle
        .triggers
        .iter()
        .filter_map(|t| {
            t.actions
                .iter()
                .find(|a| a["name"] == "Complete_Objective")
                .map(|a| {
                    (
                        a["data"]["lang"].as_str().unwrap_or(""),
                        t.done.iter().all(|d| *d),
                    )
                })
        })
        .take(3)
        .collect();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.popup_shade(&root.nodes[0], camera);
    let s = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / s - root.size[0]) * 0.5,
        (camera.height as f64 / s - root.size[1]) * 0.5,
    ];
    let mut root = root.clone();
    for n in &mut root.nodes {
        if n.id == "hint_txt" {
            n.parents.push("source_goals_hint".into());
        }
    }
    v.add(
        &root,
        o,
        s,
        |n| {
            if n.id == "arrowLeft" {
                return false;
            };
            for i in 1..=3 {
                if n.parents
                    .iter()
                    .any(|p| p == &format!("goal{i}_circle") || p == &format!("goal{i}_txt"))
                {
                    return i <= objectives.len();
                };
                if n.parents.iter().any(|p| p == &format!("goal{i}_ico")) {
                    return objectives.get(i - 1).is_some_and(|(_, done)| *done);
                };
            }
            true
        },
        |n| {
            Some(match n.id.as_str() {
                "header_txt" => {
                    interface::lang(scene, &format!("#item_{}_name", m.mission_id), &m.title).into()
                }
                "hint_txt" => {
                    interface::lang(scene, &format!("#item_{}_DESCRIPTION", m.mission_id), "")
                        .into()
                }
                "goals_txt" => interface::lang(scene, "#mission_goals", "").into(),
                id => (1..=3)
                    .find(|i| id == format!("goal{i}_txt"))
                    .and_then(|i| objectives.get(i - 1))
                    .map(|(key, _)| interface::lang(scene, key, key).into())
                    .unwrap_or_default(),
            })
        },
        None,
    );
    v.button(&root, "close_btn", o, s, Action::Close);
}
