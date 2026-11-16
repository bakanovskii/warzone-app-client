use std::{path::PathBuf, sync::Arc};
use warzone_app_client::{
    Camera, Scene,
    expansions::Payment,
    interface::View,
    interface_motion::BottomPlayback,
    production::State,
    save::{Profile, digest},
    world::expansion_position,
};

fn fixture() -> (Scene, State, String) {
    let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/pack");
    let scene = Scene::load(&pack).expect("run tools/prepare.py first");
    let seed = digest(&std::fs::read(pack.join("scene.json")).unwrap());
    let mut state = State::seed(&scene, seed.clone()).unwrap();
    state.test_only = true;
    (scene, state, seed)
}

#[test]
fn every_priced_plot_can_be_bought_picked_and_reopened() {
    let (scene, mut state, seed) = fixture();
    state.resources.insert("realMoney".into(), 1_000_000);
    let initial = state.systems.as_ref().unwrap().preferences["expansionsMap"]
        .as_object()
        .unwrap()
        .len();
    let count = scene.expansions.as_ref().unwrap().prices.len();
    assert_eq!(count, 100);
    let mut purchases = 0;
    let mut spent = 0;
    while let Some(id) = state.available_expansions(&scene).first().copied() {
        let [x, y] = expansion_position(id as i32).unwrap();
        let point = scene
            .projection
            .project((x * 6 + 3) as f64, (y * 6 + 3) as f64);
        for zoom in [0.12, 0.5, 1., 2.] {
            let camera = Camera {
                x: point[0],
                y: point[1],
                zoom,
                width: 1920,
                height: 1080,
            };
            assert_eq!(
                state.expansion_at(&scene, &camera, camera.screen(point)),
                Some(id)
            );
        }
        let price = state.expansion_quote(&scene, id).unwrap().cash;
        let (next, _) = state
            .buy_expansion(&scene, id, Payment::Cash, 1_800_000_000 + purchases)
            .unwrap();
        assert!(
            next.buy_expansion(&scene, id, Payment::Cash, 1_800_000_001 + purchases)
                .is_err()
        );
        spent += price;
        purchases += 1;
        state = next;
    }
    assert_eq!(purchases as usize + initial, count);
    assert_eq!(state.resources["realMoney"], 1_000_000 - spent);
    assert!(state.available_expansions(&scene).is_empty());
    state.validate(&scene).unwrap();
    let path = std::env::temp_dir().join(format!("warzone-land-regression-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    {
        let mut profile = Profile::open(&path, &scene, &seed, true).unwrap();
        profile.commit(&scene, state.clone()).unwrap();
    }
    {
        let profile = Profile::open(&path, &scene, &seed, true).unwrap();
        assert_eq!(profile.state.resources, state.resources);
        assert!(profile.state.available_expansions(&scene).is_empty());
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn map_only_zones_never_offer_a_dead_purchase() {
    let (scene, state, _) = fixture();
    for id in [49, 70, 95, 789, 870, 871, 956, 957, 1046] {
        assert!(!state.available_expansions(&scene).contains(&id));
        assert!(state.expansion_quote(&scene, id).is_err());
    }
}

#[test]
fn bottom_transition_blocks_its_controls_but_accepts_rapid_map_clicks() {
    let camera = Camera {
        x: 0.,
        y: 0.,
        zoom: 1.,
        width: 960,
        height: 640,
    };
    let mut motion = BottomPlayback::default();
    let mut view = View::default();
    motion.apply("none".into(), 84., &camera, 0., &mut view);
    motion.apply("selected".into(), 84., &camera, 0.01, &mut view);
    assert!(motion.active(0.02));
    for time in [0.02, 0.04, 0.08, 0.12, 0.16, 0.20, 0.26] {
        assert!(!motion.blocks_point(time, [480., 300.]));
        assert!(motion.blocks_point(time, [480., 620.]));
    }
    assert!(!motion.blocks_point(0.32, [480., 620.]));
    assert!(!view.modal);
}

#[test]
fn animation_frames_are_shared_by_entity_clones() {
    let (scene, _, _) = fixture();
    let mut checked = 0;
    for entity in &scene.entities {
        let copy = entity.clone();
        for (a, b) in entity.tracks.iter().zip(copy.tracks.iter()) {
            assert!(Arc::ptr_eq(&a.frames, &b.frames));
            if let (Some(a), Some(b)) = (&a.ambient, &b.ambient) {
                assert!(Arc::ptr_eq(a, b));
            }
            checked += 1;
        }
    }
    assert!(checked > 100);
}

#[test]
fn all_base_models_upgrades_working_and_battle_idle_use_complete_timelines() {
    let (mut scene, _, _) = fixture();
    // Match the real startup path: the initial language is applied after load.
    scene.select_language(1).unwrap();
    let check = |tracks: &[warzone_app_client::Track]| {
        for t in tracks.iter().filter(|t| t.fps > 0.) {
            assert!(
                t.ambient.is_some(),
                "Missing ambient timeline: {}",
                t.texture
            );
        }
    };
    for entity in &scene.entities {
        check(&entity.tracks);
    }
    let world = scene.world.as_ref().unwrap();
    for def in world.catalog.values() {
        for tracks in def.variants.values() {
            check(tracks);
        }
    }
    for work in world.working.values() {
        for tracks in work.normal.values().chain(work.alternate.values()) {
            check(tracks);
        }
    }
    for model in scene.upgrades.as_ref().unwrap().models.values() {
        for tracks in model.variants.values() {
            check(tracks);
        }
    }
    let combat = scene.combat.as_ref().unwrap();
    for tracks in combat
        .sprites
        .values()
        .chain(combat.views.values().flat_map(|v| v.sprites.values()))
    {
        check(tracks);
    }
    // Attacks/runs/deaths have event clocks and one-shot lifetimes, not idle phases.
    for tracks in combat.animations.values() {
        assert!(tracks.iter().all(|t| t.ambient.is_none()));
    }
}

#[test]
fn replay_button_uses_selected_language_without_losing_ambient_clips() {
    use warzone_app_client::{interface::Action, interface_campaign};
    let (mut scene, mut state, _) = fixture();
    let original = scene.clone();
    let mission_id = scene
        .combat
        .as_ref()
        .unwrap()
        .campaign()
        .next()
        .unwrap()
        .mission_id;
    state
        .systems
        .as_mut()
        .unwrap()
        .preferences
        .insert(format!("Mission-{mission_id}"), true.into());
    let camera = Camera {
        x: 0.,
        y: 0.,
        zoom: 1.,
        width: 1920,
        height: 1080,
    };
    for (language, text) in [
        (0, "Replay mission"),
        (1, "Пройти снова"),
        (2, "重玩任务"),
        (0, "Replay mission"),
    ] {
        state
            .systems
            .as_mut()
            .unwrap()
            .preferences
            .insert("langID".into(), language.into());
        scene.select_language(language).unwrap();
        let mut view = View::default();
        assert!(interface_campaign::build(
            &mut view,
            &scene,
            &state,
            &camera,
            0,
            true,
            [0., 0.],
            2.
        ));
        assert!(view.draws.iter().any(|d| matches!(
            d.node.id.as_str(),
            "startmissionbtn_txt" | "play_txt"
        ) && d.node.text == text));
        assert!(view.hits.iter().any(|h| h.action == Action::Primary));
        for (&id, def) in &original.world.as_ref().unwrap().catalog {
            for (&direction, tracks) in &def.variants {
                for (a, b) in tracks
                    .iter()
                    .zip(&scene.world.as_ref().unwrap().catalog[&id].variants[&direction])
                {
                    if let Some(clip) = &a.ambient {
                        assert!(Arc::ptr_eq(
                            clip,
                            b.ambient
                                .as_ref()
                                .expect("Language projection dropped ambient clip")
                        ));
                    }
                }
            }
        }
    }
}

#[test]
fn trooper_plays_all_fifty_idle_frames_and_flag_uses_original_fifteen_fps() {
    let (scene, _, _) = fixture();
    let world = scene.world.as_ref().unwrap();
    let tracks = &world.catalog[&2001].variants[&warzone_app_client::world::SE];
    let body = tracks
        .iter()
        .find(|t| {
            t.ambient
                .as_ref()
                .is_some_and(|c| !c.symbol.contains("_shadow_"))
        })
        .unwrap();
    let clip = body.ambient.as_ref().unwrap();
    assert_eq!(clip.fps, 8.);
    let warzone_app_client::animation::Kind::Clip { frames } = &clip.nodes[clip.roots[0]].kind
    else {
        panic!("Not a unit clip")
    };
    assert_eq!(frames.len(), 50);
    let sprites = |track: &warzone_app_client::Track, t| {
        serde_json::to_value(track.frame(t).collect::<Vec<_>>()).unwrap()
    };
    assert_ne!(sprites(body, 0.0001), sprites(body, 1.5001));
    assert_eq!(sprites(body, 0.0001), sprites(body, 6.2501));
    let flag = world
        .catalog
        .values()
        .flat_map(|d| d.variants.values())
        .flatten()
        .find(|t| {
            t.ambient
                .as_ref()
                .is_some_and(|c| c.symbol == "bs_animations_001_barracks_se_idle")
        })
        .unwrap();
    assert_eq!(flag.ambient.as_ref().unwrap().fps, 15.);
    assert_eq!(sprites(flag, 0.0001), sprites(flag, 20. / 15. + 0.0001));
    assert_ne!(sprites(flag, 0.0001), sprites(flag, 2.0001));
}

#[test]
fn independent_object_clocks_survive_world_rebuilds_and_large_time_values() {
    use warzone_app_client::animation::idle_time;
    let (scene, state, _) = fixture();
    let a = state.world_entities(&scene).unwrap();
    let b = state.world_entities(&scene).unwrap();
    let units: Vec<_> = a.iter().filter(|e| e.type_id == 2001).collect();
    let times: std::collections::BTreeSet<_> = units
        .iter()
        .map(|e| (idle_time(&e.uid, 0.) * 8.) as u64 % 50)
        .collect();
    assert!(
        times.len() > 1,
        "Identical squads must not share an idle phase"
    );
    for entity in a
        .iter()
        .filter(|e| e.tracks.iter().any(|t| t.ambient.is_some()))
    {
        let same = b.iter().find(|e| e.uid == entity.uid).unwrap();
        assert_eq!(idle_time(&entity.uid, 217.71), idle_time(&same.uid, 217.71));
        assert!(
            (idle_time(&entity.uid, 217.71) - idle_time(&entity.uid, 217.70) - 0.01).abs() < 1e-10
        );
        for (a, b) in entity.tracks.iter().zip(&same.tracks) {
            if let (Some(a), Some(b)) = (&a.ambient, &b.ambient) {
                assert!(Arc::ptr_eq(a, b));
            }
        }
    }
}

#[test]
fn mission_briefing_retains_the_campaign_map_and_its_offset() {
    use warzone_app_client::{interface::Action, interface_campaign};
    let (scene, state, _) = fixture();
    for (width, height) in [(960, 640), (1920, 1080)] {
        let camera = Camera {
            x: 0.,
            y: 0.,
            zoom: 1.,
            width,
            height,
        };
        let offset = [75., 210.];
        let mut map = View::default();
        let mut briefing = View::default();
        assert!(interface_campaign::build(
            &mut map, &scene, &state, &camera, 0, false, offset, 1.25,
        ));
        assert!(interface_campaign::build(
            &mut briefing,
            &scene,
            &state,
            &camera,
            0,
            true,
            offset,
            1.25,
        ));
        assert!(briefing.draws.len() > map.draws.len());
        assert_eq!(
            serde_json::to_value(&briefing.draws[..map.draws.len()]).unwrap(),
            serde_json::to_value(&map.draws).unwrap(),
        );
        assert!(
            briefing
                .hits
                .iter()
                .all(|hit| matches!(hit.action, Action::Close | Action::Primary))
        );
    }
}

#[test]
fn starting_a_campaign_without_troops_returns_the_original_warning() {
    use warzone_app_client::mission::Command;
    let (scene, mut state, _) = fixture();
    let combat = scene.combat.as_ref().unwrap();
    for troop in state.mission_roster(&scene) {
        state = state.demolish(&scene, &troop.uid, 1).unwrap().0;
    }
    let name = combat.campaign_names().next().unwrap().clone();
    let parent = combat.missions[&name].parent;
    state
        .systems
        .as_mut()
        .unwrap()
        .set(format!("Mission-{parent}"), 1);
    state.validate(&scene).unwrap();
    assert!(state.mission_roster(&scene).is_empty());
    assert!(state.mission_access(&scene, &name).is_ok());
    assert_eq!(
        state
            .mission_apply(&scene, &Command::Open(name), 1)
            .err()
            .unwrap(),
        "У тебя нет армии! Отправляйся на базу и начни тренировку войск.",
    );
    assert!(state.mission.is_none());
}

#[test]
fn wide_panels_cover_the_viewport_and_controls_follow_their_art() {
    use warzone_app_client::interface::{self, Action, Screen};
    let (scene, state, _) = fixture();
    for (width, height) in [(1420, 800), (1910, 910), (2560, 1080)] {
        let camera = Camera {
            x: 0.,
            y: 0.,
            zoom: 1.,
            width,
            height,
        };
        for screen in [Screen::Inventory, Screen::Settings] {
            let view = interface::build(
                &scene,
                &state,
                &camera,
                Some(screen),
                None,
                false,
                0.,
                false,
            )
            .unwrap();
            assert!(view.draws.iter().any(|d| d.node.kind == "image"
                && d.node.id.is_empty()
                && d.node.rect[0] <= 1.
                && d.node.rect[0] + d.node.rect[2] >= width as f64 - 1.));
            let close = view
                .hits
                .iter()
                .find(|h| h.action == Action::Close)
                .unwrap();
            assert!(close.rect[0] > width as f64 * 0.8);
            assert_eq!(
                view.hit(
                    (close.rect[0] + close.rect[2] / 2.) as i32,
                    (close.rect[1] + close.rect[3] / 2.) as i32
                ),
                Some(Action::Close)
            );
        }
        for panel in ["achievements", "leagues", "leagues_help", "pvp"] {
            let mut view = View::default();
            if panel == "achievements" {
                warzone_app_client::interface_achievements::build(
                    &mut view, &scene, &state, &camera, 0.,
                )
                .unwrap();
            } else if panel == "leagues" {
                warzone_app_client::interface_leagues::build(&mut view, &scene, &state, &camera, 0)
                    .unwrap();
            } else if panel == "leagues_help" {
                warzone_app_client::interface_leagues::help(&mut view, &scene, &state, &camera, 0.)
                    .unwrap();
            } else {
                warzone_app_client::interface_pvp::profile(&mut view, &scene, &state, &camera)
                    .unwrap();
            }
            assert!(view.draws.iter().any(|d| d.node.kind == "image"
                && d.node.id.is_empty()
                && d.node.rect[0] <= 1.
                && d.node.rect[0] + d.node.rect[2] >= width as f64 - 1.));
            let close = view
                .hits
                .iter()
                .find(|h| h.action == Action::Close)
                .unwrap();
            assert!(close.rect[0] > width as f64 * 0.8, "{panel}");
            assert_eq!(
                view.hit(
                    (close.rect[0] + close.rect[2] / 2.) as i32,
                    (close.rect[1] + close.rect[3] / 2.) as i32
                ),
                Some(Action::Close)
            );
        }
        let root = interface::wide_panel(
            &scene.interface.as_ref().unwrap().symbols["deployment"],
            &camera,
            &["battlestart_btn", "battlestart_txt", "battlereturn_btn"],
        );
        let scale = warzone_app_client::hud::scale(&camera);
        assert!((root.size[0] * scale - width as f64).abs() < 0.01);
        assert!(root.buttons["battlestart_btn"][0] * scale > width as f64 * 0.75);
    }
}

#[test]
fn academy_trees_cover_wide_and_tall_viewports_while_scrolling() {
    let (scene, state, _) = fixture();
    for (width, height) in [
        (960, 640),
        (1920, 1080),
        (2560, 1080),
        (844, 390),
        (390, 844),
    ] {
        let camera = Camera {
            x: 0.,
            y: 0.,
            zoom: 1.,
            width,
            height,
        };
        for unit in &scene.upgrades.as_ref().unwrap().unit_order {
            for scroll in [0., 500.] {
                let mut view = View::default();
                assert!(warzone_app_client::interface_upgrades::build(
                    &mut view,
                    &scene,
                    &state,
                    &camera,
                    Some(*unit),
                    None,
                    false,
                    scroll,
                ));
                let background = view
                    .draws
                    .iter()
                    .find(|d| d.node.id == "background")
                    .unwrap();
                assert_eq!(
                    background.node.rect,
                    [0., 0., width as f64, height as f64],
                    "unit {unit}, {width}x{height}, scroll {scroll}"
                );
                let close = view
                    .hits
                    .iter()
                    .find(|h| h.action == warzone_app_client::interface::Action::Close)
                    .unwrap();
                assert!(close.rect[0] + close.rect[2] <= width as f64);
                assert!(close.rect[1] + close.rect[3] <= height as f64);
            }
        }
    }
}

#[test]
fn right_side_story_keeps_its_distance_from_the_right_edge() {
    let (scene, state, _) = fixture();
    let mut previous: Option<f64> = None;
    let mut balloon_edge: Option<f64> = None;
    for width in [1420, 1920, 2560] {
        let camera = Camera {
            x: 0.,
            y: 0.,
            zoom: 1.,
            width,
            height: 800,
        };
        let mut view = View::default();
        warzone_app_client::interface_story::add_message(
            &mut view,
            &scene,
            &state,
            &camera,
            "#dialog_stein_right",
            "В атаку!",
        )
        .unwrap();
        let portrait = view
            .draws
            .iter()
            .find(|d| d.node.parents.iter().any(|p| p == "source_story_portrait"))
            .unwrap();
        let distance = width as f64 - portrait.node.rect[0] - portrait.node.rect[2];
        if let Some(previous) = previous {
            assert!((distance - previous).abs() < 0.01);
        }
        previous = Some(distance);
        let text = view.draws.iter().find(|d| d.node.id == "hint_txt").unwrap();
        let edge = width as f64 - text.node.rect[0] - text.node.rect[2];
        if let Some(previous) = balloon_edge {
            assert!((edge - previous).abs() < 0.01);
        }
        balloon_edge = Some(edge);
    }
}
