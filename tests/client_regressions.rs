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
fn campaign_oil_reward_does_not_leave_a_stale_counter_on_base() {
    use warzone_app_client::{
        battle::Battle,
        mission::Session,
        production::{Action, ProductionSource},
        reward_effects::Playback,
    };
    let (scene, mut state, _) = fixture();
    let battle = Battle::new(scene.combat.as_ref().unwrap(), "mission_1", vec![]).unwrap();
    let target = battle.units.iter().find(|u| u.kind == "building").unwrap();
    let mut playback = Playback::default();
    playback.initialize(state.revision);
    state.revision += 1;
    state.resources.insert("fuel".into(), 135);
    state.actions.push(Action {
        shop: None,
        source: Some(ProductionSource {
            uid: "mission-test".into(),
            started: 0,
        }),
        sequence: state.revision,
        at: 0,
        at_ms: None,
        uid: target.uid.clone(),
        recipe: target.id,
        kind: "mission_destroy_bonus".into(),
        gains: [("fuel".into(), 3)].into(),
        entropy: vec![],
        quest_events_pending: vec![],
    });

    // The base can see the receipt for the first time after leaving the mission.
    playback.update(&scene, &state, 0.);
    assert_eq!(playback.hud_values(&scene, &state, 0).texts["fuel"], "135");

    playback.initialize(state.revision - 1);
    state.mission = Some(Session {
        id: "mission-test".into(),
        opened: 0,
        reward_eligible: true,
        camera: scene.camera,
        camera_serial: 0,
        battle,
    });
    playback.update(&scene, &state, 0.);
    assert_eq!(playback.hud_values(&scene, &state, 0).texts["fuel"], "132");
    state.mission = None;
    playback.update(&scene, &state, 0.1);
    assert_eq!(playback.hud_values(&scene, &state, 0).texts["fuel"], "135");
}

#[test]
fn battle_target_cells_are_aligned_on_every_map_and_zoom() {
    use std::collections::BTreeMap;
    use warzone_app_client::battle::Battle;
    let (scene, _, _) = fixture();
    let combat = scene.combat.as_ref().unwrap();
    let mut checked = 0;
    for mission in combat.missions.keys() {
        let battle = Battle::new(combat, mission, vec![]).unwrap();
        let view = battle
            .render_scene_with_models(&scene, &BTreeMap::new())
            .unwrap();
        for entity in &view.entities {
            if [1026, 1028].contains(&entity.type_id) {
                continue; // Resource fields resolve to their harvester, not themselves.
            }
            let anchor = view.projection.project(entity.x, entity.y);
            let points = if entity.kind == "unit" {
                vec![
                    [anchor[0] - 20., anchor[1] + 15.],
                    [anchor[0] + 20., anchor[1] + 15.],
                ]
            } else {
                let [x, y, w, h] = entity.footprint();
                vec![
                    // This lower corner was outside the shifted lean-view hitbox.
                    view.projection.project(x + w - 0.1, y + h - 0.1),
                    view.projection.project(x + 0.1, y + 0.1),
                ]
            };
            for zoom in [0.12, 0.5, 1., 2.] {
                let camera = Camera {
                    x: anchor[0],
                    y: anchor[1],
                    zoom,
                    width: 1920,
                    height: 1080,
                };
                for point in &points {
                    let picked = view.pick_battle_filtered_positions(
                        &camera,
                        camera.screen(*point),
                        |e| e.uid == entity.uid,
                        |e| [e.x, e.y],
                    );
                    assert_eq!(
                        picked.map(|i| &view.entities[i].uid),
                        Some(&entity.uid),
                        "{}: {} facing {} at {zoom}",
                        mission,
                        entity.uid,
                        entity.direction
                    );
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 500, "Only checked {checked} targets");
}

#[test]
fn battle_picking_preserves_neighbors_filters_and_moving_poses() {
    use std::collections::BTreeMap;
    use warzone_app_client::battle::Battle;
    let (scene, _, _) = fixture();
    let battle = Battle::new(scene.combat.as_ref().unwrap(), "mission_1", vec![]).unwrap();
    let mut view = battle
        .render_scene_with_models(&scene, &BTreeMap::new())
        .unwrap();
    let mut first = view
        .entities
        .iter()
        .find(|e| e.kind == "unit")
        .unwrap()
        .clone();
    first.x = 10.;
    first.y = 10.;
    first.uid = "front".into();
    let mut neighbor = first.clone();
    neighbor.x = 11.;
    neighbor.uid = "neighbor".into();
    view.entities = vec![neighbor, first];
    let camera = Camera {
        x: 0.,
        y: 0.,
        zoom: 0.5,
        width: 1920,
        height: 1080,
    };
    let anchor = view.projection.project(11., 10.);
    let click = camera.screen([anchor[0], anchor[1] + 15.]);
    let pick = |point, accept: fn(&warzone_app_client::Entity) -> bool| {
        view.pick_battle_filtered_positions(&camera, point, accept, |e| [e.x, e.y])
    };
    assert_eq!(
        pick(click, |_| true),
        Some(0),
        "Front target stole a neighbor hit"
    );
    assert_eq!(pick(click, |e| e.uid == "front"), None);
    assert_eq!(pick(click, |_| false), None);
    let anchor = view.projection.project(15., 16.);
    let click = camera.screen([anchor[0], anchor[1] + 15.]);
    assert_eq!(
        pick(click, |_| true),
        None,
        "Distant empty ground must remain empty"
    );
    assert_eq!(
        view.pick_battle_filtered_positions(&camera, click, |e| e.uid == "front", |_| [15., 16.]),
        Some(1)
    );
    // Catalog retention must not change the interactive battle hitbox.
    view.combat = scene.combat.clone();
    assert_eq!(
        view.pick_battle_filtered_positions(&camera, click, |e| e.uid == "front", |_| [15., 16.]),
        Some(1)
    );
}

#[test]
fn interrupted_saves_recover_only_valid_compatible_backups_without_resetting() {
    use std::fs;
    let (scene, _, seed) = fixture();
    let root = std::env::temp_dir().join(format!("warzone-recovery-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for fault in [
        "truncated",
        "checksum",
        "missing",
        "both",
        "all",
        "future",
        "seed",
        "stale",
    ] {
        let path = root.join(fault);
        let starting;
        {
            let mut profile = Profile::open(&path, &scene, &seed, true).unwrap();
            starting = profile.state.resources["realMoney"];
            let mut state = profile.state.clone();
            state.resources.insert("realMoney".into(), starting + 10);
            profile.commit(&scene, state).unwrap();
        }
        {
            // Save a session checkpoint at +10, then two distinct revisions.
            let mut profile = Profile::open(&path, &scene, &seed, true).unwrap();
            for amount in [20, 30] {
                let mut state = profile.state.clone();
                state
                    .resources
                    .insert("realMoney".into(), starting + amount);
                profile.commit(&scene, state).unwrap();
            }
        }
        let original = fs::read(path.join("state.json")).unwrap();
        match fault {
            "missing" => fs::remove_file(path.join("state.json")).unwrap(),
            "checksum" => {
                let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
                json["state"]["resources"]["realMoney"] = 999999.into();
                fs::write(path.join("state.json"), serde_json::to_vec(&json).unwrap()).unwrap();
            }
            "future" | "seed" => {
                let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
                if fault == "future" {
                    json["format"] = "warzone-desktop-save-v999".into();
                } else {
                    let mut state: State = serde_json::from_value(json["state"].take()).unwrap();
                    state.seed_sha256 = "different-seed".into();
                    json["sha256"] = digest(&serde_json::to_vec(&state).unwrap()).into();
                    json["state"] = serde_json::to_value(state).unwrap();
                }
                fs::write(path.join("state.json"), serde_json::to_vec(&json).unwrap()).unwrap();
            }
            "stale" => {
                for name in ["state.json", "state.bak"] {
                    fs::write(
                        path.join(format!(".{name}.{}.tmp", std::process::id())),
                        b"interrupted",
                    )
                    .unwrap();
                }
            }
            _ => {
                fs::write(path.join("state.json"), b"{\"format\":").unwrap();
                if matches!(fault, "both" | "all") {
                    fs::write(path.join("state.bak"), b"damaged backup").unwrap();
                }
                if fault == "all" {
                    fs::write(path.join("state.checkpoint.bak"), b"damaged checkpoint").unwrap();
                }
            }
        }
        let broken = fs::read(path.join("state.json")).ok();
        let result = Profile::open(&path, &scene, &seed, true);
        if matches!(fault, "all" | "future" | "seed") {
            assert!(result.is_err(), "{fault} was silently reset/replaced");
            assert_eq!(fs::read(path.join("state.json")).ok(), broken);
            continue;
        }
        let mut profile = result.unwrap_or_else(|error| panic!("{fault}: {error}"));
        let expected = if fault == "both" {
            10
        } else if fault == "stale" {
            30
        } else {
            20
        };
        assert_eq!(
            profile.state.resources["realMoney"],
            starting + expected,
            "{fault}"
        );
        assert_eq!(profile.recovered_from.is_some(), fault != "stale");
        if fault != "stale" && fault != "missing" {
            let quarantined: Vec<_> = fs::read_dir(&path)
                .unwrap()
                .map(Result::unwrap)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".state.damaged.")
                })
                .collect();
            assert_eq!(quarantined.len(), 1, "{fault}");
            assert_eq!(fs::read(quarantined[0].path()).unwrap(), broken.unwrap());
        }
        // Recovery must also support the next durable write and a clean reopen.
        let mut state = profile.state.clone();
        state.resources.insert("realMoney".into(), starting + 40);
        profile.commit(&scene, state).unwrap();
        drop(profile);
        let reopened = Profile::open(&path, &scene, &seed, true).unwrap();
        assert_eq!(reopened.state.resources["realMoney"], starting + 40);
        assert!(reopened.recovered_from.is_none());
    }
    fs::remove_dir_all(root).unwrap();
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
    let upgrades = scene.upgrades.as_ref().unwrap();
    let (mut dashed_links, mut round_turns) = (0, 0);
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
        for unit in &upgrades.unit_order {
            let layers = upgrades.layers(*unit).unwrap();
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
                dashed_links += view
                    .draws
                    .iter()
                    .filter(|d| d.node.id == "academy_connector_dash")
                    .count();
                round_turns += view
                    .draws
                    .iter()
                    .filter(|d| {
                        d.node.id == "academy_connector_turn" && d.node.rect[2] == d.node.rect[3]
                    })
                    .count();
                let backdrop = |id| {
                    let group = format!("upgrade_cell_{id}");
                    view.draws
                        .iter()
                        .find(|d| d.node.parents.first() == Some(&group))
                        .unwrap()
                        .node
                        .rect
                };
                for pair in layers.windows(2) {
                    if pair[0].len() != 1
                        || pair[1].len() != 3
                        || upgrades.items[&pair[0][0]].model(*unit).is_none()
                    {
                        continue;
                    }
                    let [x, y, w, h] = backdrop(pair[0][0]);
                    let turns = view
                        .draws
                        .iter()
                        .filter_map(|d| {
                            let [tx, ty, tw, th] = d.node.rect;
                            (d.node.id == "academy_connector_turn"
                                && (tx + tw / 2. - (x + w * 0.77)).abs() < 0.001)
                                .then_some(ty + th / 2.)
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        turns.iter().any(|&ty| ty < y),
                        "Missing upper turn for unit {unit}"
                    );
                    assert!(
                        turns.iter().any(|&ty| ty > y + h),
                        "Missing lower turn for unit {unit}"
                    );
                }
                for layer in &layers {
                    let mut cards: Vec<_> = layer.iter().map(|id| backdrop(*id)).collect();
                    cards.sort_by(|a, b| a[1].total_cmp(&b[1]));
                    for card in &cards {
                        let lane = (card[1] + card[3] / 2.) / (height as f64 / 3.) - 0.5;
                        assert!((0. ..=2.).contains(&lane.round()));
                        assert!((lane - lane.round()).abs() < 0.001);
                        if cards.len() == 1 {
                            assert!((lane - 1.).abs() < 0.001);
                        }
                    }
                    assert!(
                        cards
                            .windows(2)
                            .all(|pair| pair[0][1] + pair[0][3] <= pair[1][1])
                    );
                    for id in layer {
                        let [x, y, w, h] = backdrop(*id);
                        let scale = warzone_app_client::hud::scale(&camera);
                        let gap = 6. * scale;
                        let endpoints = view
                            .draws
                            .iter()
                            .filter(|d| {
                                let [ax, ay, aw, ah] = d.node.rect;
                                d.node.id == "academy_connector_arrow"
                                    && if aw > ah {
                                        (ax + aw + gap - x).abs() < 0.001
                                            && ay >= y
                                            && ay + ah <= y + h
                                    } else {
                                        ax >= x
                                            && ax + aw <= x + w
                                            && ((ay + ah + gap - y).abs() < 0.001
                                                || (ay - gap - y - h).abs() < 0.001)
                                    }
                            })
                            .count();
                        assert_eq!(
                            endpoints,
                            upgrades.items[id].parents.len(),
                            "unit {unit}, upgrade {id}"
                        );
                    }
                }
                // A third prerequisite may feed into the row above it. The
                // two existing tracks must not move just because that row ends.
                let center = |id| {
                    let r = backdrop(id);
                    r[1] + r[3] / 2.
                };
                // The branch order is a presentation choice, separate from
                // prerequisite order. Keep the entire chain in the same row.
                match unit {
                    2005 => {
                        assert!(center(11113) < center(11112));
                        assert_eq!(center(11113), center(11115));
                        assert_eq!(center(11112), center(11114));
                    }
                    2009 => {
                        assert!(center(11127) < center(11126));
                        assert!(center(11126) < center(11128));
                        assert_eq!(center(11127), center(11135));
                        assert_eq!(center(11126), center(11129));
                    }
                    2011 => {
                        assert!(center(11183) < center(11184));
                        assert!(center(11184) < center(11055));
                        assert_eq!(center(11183), center(11186));
                        assert_eq!(center(11055), center(11185));
                    }
                    _ => {}
                }
                let continuations = match unit {
                    2005 => [(11009, 11111, 11014), (11010, 11013, 11015)],
                    2007 => [(11024, 11117, 11029), (11025, 11028, 11030)],
                    2017 => [(11017, 11105, 11022), (11018, 11021, 11110)],
                    _ => continue,
                };
                for (start, linked, end) in continuations {
                    assert!((center(start) - center(linked)).abs() < 0.001);
                    assert!((center(linked) - center(end)).abs() < 0.001);
                }
            }
        }
    }
    assert!(dashed_links > 0 && round_turns > 0);
}

#[test]
fn academy_lists_the_tank_discount_before_the_production_time_change() {
    let (mut scene, _, _) = fixture();
    for (language, expected) in [
        (1, "Экономит 20%\nПроизводство: −1 час"),
        (0, "Reduces costs 20%\nProduction: −1 hour"),
        (2, "减少成本 20%\n生产: −1 小时"),
    ] {
        scene.select_language(language).unwrap();
        let item = &scene.upgrades.as_ref().unwrap().items[&11138];
        assert_eq!(
            warzone_app_client::interface_upgrades::description(&scene, item)
                .replace('\u{a0}', " "),
            expected
        );
    }
}

#[test]
fn academy_training_effects_wrap_inside_the_card_without_splitting_the_duration() {
    let (mut scene, state, _) = fixture();
    scene.select_language(1).unwrap();
    let camera = Camera {
        x: 0.,
        y: 0.,
        zoom: 1.,
        width: 1920,
        height: 1080,
    };
    let scale = warzone_app_client::hud::scale(&camera);
    for (unit, upgrade) in [(2005, 11010), (2017, 11106)] {
        let mut view = View::default();
        assert!(warzone_app_client::interface_upgrades::build(
            &mut view,
            &scene,
            &state,
            &camera,
            Some(unit),
            None,
            false,
            0.,
        ));
        let group = format!("upgrade_cell_{upgrade}");
        let nodes: Vec<_> = view
            .draws
            .iter()
            .filter(|d| d.node.parents.first() == Some(&group))
            .collect();
        let card = nodes[0].node.rect;
        let label = &nodes
            .iter()
            .find(|d| d.node.id == "upgrade_modifires_txt")
            .unwrap()
            .node;
        assert!(label.multiline);
        assert!(label.text.ends_with("−3\u{a0}минуты"));
        assert!(label.rect[0] + label.rect[2] <= card[0] + card[2] - 7. * scale + 0.001);
    }
}

#[test]
fn academy_completion_uses_the_full_progress_track_only_at_100_percent() {
    use warzone_app_client::interface_upgrades::build;
    let (scene, mut state, _) = fixture();
    let upgrades = scene.upgrades.as_ref().unwrap();
    let unit = upgrades.unit_order[0];
    let ids: Vec<_> = upgrades
        .layers(unit)
        .unwrap()
        .into_iter()
        .flatten()
        .collect();
    let camera = Camera {
        x: 0.,
        y: 0.,
        zoom: 1.,
        width: 960,
        height: 640,
    };
    let group = format!("upgrades_unit_{unit}");
    for completed in [0, 1, ids.len()] {
        let systems = state.systems.as_mut().unwrap();
        for (i, id) in ids.iter().enumerate() {
            systems.set(format!("Upgrade-{id}"), i64::from(i < completed));
        }
        let mut view = View::default();
        assert!(build(
            &mut view, &scene, &state, &camera, None, None, false, 0.
        ));
        let draw = |id| {
            view.draws
                .iter()
                .find(|d| d.node.id == id && d.node.parents.first() == Some(&group))
                .unwrap()
        };
        let bar = draw("progressbar");
        let frame = draw("progress").node.rect;
        assert_eq!(bar.progress, completed as f64 / ids.len() as f64);
        assert_eq!(
            draw("progress_text").node.text,
            format!("{}%", completed * 100 / ids.len())
        );
        let left = bar.node.rect[0] - frame[0];
        let right = frame[0] + frame[2] - bar.node.rect[0] - bar.node.rect[2] * bar.progress;
        if completed == ids.len() {
            assert!(
                (right - left).abs() < 0.001,
                "The completed fill should reach both inner edges"
            );
        } else {
            assert!(right > left);
        }
    }
}

#[test]
fn academy_upgrade_dialogs_keep_the_scrolled_tree_behind_them() {
    use warzone_app_client::{interface::Action, interface_upgrades::build};
    let (scene, state, _) = fixture();
    let upgrades = scene.upgrades.as_ref().unwrap();
    let unit = upgrades.unit_order[0];
    let upgrade = upgrades.layers(unit).unwrap()[0][0];
    let camera = Camera {
        x: 0.,
        y: 0.,
        zoom: 1.,
        width: 1920,
        height: 1080,
    };
    let scroll = 240.;
    let mut tree = View::default();
    assert!(build(
        &mut tree,
        &scene,
        &state,
        &camera,
        Some(unit),
        None,
        false,
        scroll
    ));
    for info in [false, true] {
        let mut dialog = View::default();
        assert!(build(
            &mut dialog,
            &scene,
            &state,
            &camera,
            Some(unit),
            Some(upgrade),
            info,
            scroll
        ));
        assert!(dialog.modal);
        assert_eq!(dialog.scroll_max, tree.scroll_max);
        assert_eq!(dialog.draws[tree.draws.len()].node.id, "source_popup_shade");
        for (actual, expected) in dialog.draws.iter().zip(&tree.draws) {
            assert_eq!(actual.node.id, expected.node.id);
            assert_eq!(actual.node.rect, expected.node.rect);
        }
        assert!(dialog.hits.iter().any(|h| h.action == Action::Close));
        assert!(
            !dialog
                .hits
                .iter()
                .any(|h| matches!(h.action, Action::UpgradeOpen(_) | Action::UpgradeInfo(_)))
        );
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
