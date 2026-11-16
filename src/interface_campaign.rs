//! Original MissionsDialog map and MissionInfoDialog poster. The authored map
//! pivots and source placement constants are independent of the base camera.
use crate::{
    Camera, Scene,
    interface::{self, Action, Hit, Symbol, View},
    production::State,
};

fn reset(v: &mut View) {
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
}
pub fn available(scene: &Scene) -> bool {
    scene
        .interface
        .as_ref()
        .is_some_and(|c| c.symbols.contains_key("campaign_map"))
}
pub fn initial_offset() -> [f64; 2] {
    [790. / 6., 692. * 0.5 - 30.]
}
pub fn clamp_offset(camera: &Camera, offset: [f64; 2]) -> [f64; 2] {
    let s = crate::hud::scale(camera);
    [
        offset[0].clamp(0., (790. - camera.width as f64 / s).max(0.)),
        offset[1].clamp(0., (692. - camera.height as f64 / s).max(0.)),
    ]
}
fn marker_origin(symbol: &Symbol, center: [f64; 2], pivot: [f64; 2], offset: [f64; 2]) -> [f64; 2] {
    [
        center[0] - symbol.size[0] * 0.5 - pivot[0] - offset[0],
        center[1] - symbol.size[1] * 0.5 - pivot[1] - offset[1],
    ]
}
pub fn build(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    selected: usize,
    briefing: bool,
    offset: [f64; 2],
    seconds: f64,
) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(combat) = &scene.combat else {
        return false;
    };
    let Some(root) = c.symbols.get("campaign_map") else {
        return false;
    };
    let s = crate::hud::scale(camera);
    reset(v);
    let offset = clamp_offset(camera, offset);
    v.add(root, [-offset[0], -offset[1]], s, |_| true, |_| None, None);
    let next = state
        .mission
        .as_ref()
        .filter(|_| state.mission_paused())
        .and_then(|session| {
            combat
                .campaign_names()
                .position(|name| name == &session.battle.mission)
        })
        .or_else(|| {
            combat.campaign().position(|m| {
                state.mission_access(scene, &m.mission).is_ok()
                    && !state
                        .systems
                        .as_ref()
                        .is_some_and(|p| p.flag(&format!("Mission-{}", m.mission_id)))
            })
        });
    let mut callout = None;
    for (index, m) in combat.campaign().enumerate() {
        let (key, pivot, raise) = match m.map_marker.as_str() {
            "bc_gui_020_map_marker_big" => ("campaign_missionBigItem", [-17., -64.], 30.),
            "bc_gui_020_map_marker_zombie" => ("campaign_missionZombieItem", [-26., -56.], 30.),
            _ => ("campaign_missionItem", [-19.5, -18.5], 3.),
        };
        let t = &c.symbols[key];
        let center = [
            453. + m.map_position[0] as f64 * 0.5,
            343. + m.map_position[1] as f64 * 0.5 - raise,
        ];
        let o = marker_origin(t, center, pivot, offset);
        let complete = state
            .systems
            .as_ref()
            .is_some_and(|p| p.flag(&format!("Mission-{}", m.mission_id)));
        let button = if complete {
            "buttonBlue"
        } else if state.mission_access(scene, &m.mission).is_ok() {
            "buttonRed"
        } else {
            "buttonBlack"
        };
        v.add(
            t,
            o,
            s,
            |n| n.parents.iter().any(|p| p == button),
            |_| None,
            None,
        );
        v.button(t, button, o, s, Action::MissionSelect(index));
        if next == Some(index) {
            callout = Some((index, center));
        }
    }
    // MissionsDialog appends nextMission only after every marker. Its drawing
    // and tap recognizer must both win overlaps with adjacent locked markers.
    if let Some((index, center)) = callout {
        let resuming = state.mission_paused()
            && combat.campaign().nth(index).is_some_and(|m| {
                state
                    .mission
                    .as_ref()
                    .is_some_and(|session| session.battle.mission == m.mission)
            });
        let t = if resuming {
            c.symbols
                .get("campaign_resumeMission")
                .unwrap_or(&c.symbols["campaign_nextMission"])
        } else {
            &c.symbols["campaign_nextMission"]
        };
        let bob = (seconds.rem_euclid(2.) - 1.).abs() * 10.;
        let o = marker_origin(
            t,
            [center[0], center[1] - 45. - bob],
            [-41.8375, -48.4375],
            offset,
        );
        v.add(
            t,
            o,
            s,
            |_| true,
            |n| {
                Some(
                    if n.id == "nextmission_txt" || n.id == "resumemission_txt" {
                        if state.mission_paused()
                            && combat.campaign().nth(index).is_some_and(|m| {
                                state
                                    .mission
                                    .as_ref()
                                    .is_some_and(|session| session.battle.mission == m.mission)
                            })
                        {
                            interface::lang(scene, "#resume_mission", "ПРОДОЛЖИТЬ МИССИЮ").into()
                        } else {
                            interface::lang(scene, "#next_mission", "НОВАЯ МИССИЯ").into()
                        }
                    } else {
                        String::new()
                    },
                )
            },
            None,
        );
        // Its source onStartTouch recognizer opens the same mission.
        v.hits.push(Hit {
            action: Action::MissionSelect(index),
            rect: [(o[0] - 39.) * s, (o[1] - 62.5) * s, 78. * s, 77. * s],
        });
    }
    let menu = &c.symbols["campaign_menu"];
    let o = [camera.width as f64 / s, camera.height as f64 / s];
    v.add(menu, o, s, |_| true, |_| None, None);
    v.button(menu, "mainmenu_btn", o, s, Action::Close);
    if briefing {
        // The info dialog belongs to MissionsDialog. Keep the same campaign
        // offset underneath it, but prevent taps from reaching its markers/menu.
        v.hits.clear();
        v.blocks.clear();
        return build_briefing(v, scene, state, camera, selected);
    }
    true
}

fn build_briefing(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    selected: usize,
) -> bool {
    let c = scene.interface.as_ref().unwrap();
    let combat = scene.combat.as_ref().unwrap();
    let s = crate::hud::scale(camera);
    let Some(m) = combat.campaign().nth(selected) else {
        return false;
    };
    let root = &c.symbols["campaign_briefing"];
    let o = [
        (camera.width as f64 / s - root.size[0]) * 0.5,
        (camera.height as f64 / s - root.size[1]) * 0.5,
    ];
    let locked = state.mission_access(scene, &m.mission).is_err();
    // Current original normal missions have no purchase rows. Source hides
    // all empty resource/equipment areas, keeps the soldiers poster, and
    // disables Start (hides access button) for an unmet predecessor.
    v.add(
        root,
        o,
        s,
        |n| {
            !(n.id == "needed_txt"
                && state
                    .systems
                    .as_ref()
                    .is_some_and(|p| p.flag(&format!("MissionStarted-{}", m.mission_id))))
                && !n.parents.iter().any(|id| {
                    matches!(
                        id.as_str(),
                        "play_money"
                            | "oil"
                            | "iron"
                            | "button_access"
                            | "equipmentrequired_txt"
                            | "itemsArea1"
                            | "itemsArea2"
                            | "itemsArea3"
                    )
                })
                && (locked
                    || !matches!(
                        n.id.as_str(),
                        "completefirst_txt" | "completefirst_headertxt"
                    ))
        },
        |n| {
            Some(match n.id.as_str() {
                "caption_txt" => {
                    interface::lang(scene, &format!("#item_{}_name", m.mission_id), &m.title).into()
                }
                "description_txt" => {
                    interface::lang(scene, &format!("#item_{}_DESCRIPTION", m.mission_id), "")
                        .into()
                }
                "startmissionbtn_txt" | "play_txt" => {
                    if state
                        .systems
                        .as_ref()
                        .is_some_and(|p| p.flag(&format!("Mission-{}", m.mission_id)))
                    {
                        match state.language_id() {
                            0 => "Replay mission",
                            2 => "重玩任务",
                            _ => "Пройти снова",
                        }
                        .into()
                    } else {
                        interface::lang(scene, "#start_mission", "Начать миссию").into()
                    }
                }
                "needed_txt" => interface::lang(scene, "#mission_required_1", &n.text).into(),
                "completefirst_headertxt" => {
                    interface::lang(scene, "#complete_this_mission_first", "").into()
                }
                "completefirst_txt" => combat
                    .campaign()
                    .find(|p| p.mission_id == m.parent)
                    .map(|p| {
                        interface::lang(scene, &format!("#item_{}_name", p.mission_id), &p.title)
                            .to_owned()
                    })
                    .unwrap_or_default(),
                _ => String::new(),
            })
        },
        None,
    );
    if !locked {
        v.button(root, "button_play", o, s, Action::Primary);
    } else {
        for d in &mut v.draws {
            if d.node.parents.iter().any(|p| p == "button_play") {
                d.grayscale = true;
            }
        }
    }
    v.button(root, "close_btn", o, s, Action::Close);
    true
}
