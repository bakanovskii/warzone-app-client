mod audio;
#[cfg(target_os = "emscripten")]
mod browser_audio;
mod editor;
mod frame_metrics;
mod input_events;
mod render;
mod tactical;
mod ui;
use sdl2::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
    mouse::{MouseButton, MouseState, MouseWheelDirection},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use warzone_app_client::{
    Camera, Scene,
    save::{self, Profile},
};

#[derive(Default)]
struct Input {
    drag: Option<(i32, i32, bool)>,
    selected: Option<usize>,
    pans: usize,
    zooms: usize,
    picks: usize,
    resizes: usize,
    panel_press: Option<u8>,
}
impl Input {
    fn event(&mut self, event: Event, camera: &mut Camera, scene: &Scene) -> bool {
        match event {
            Event::Quit { .. } => return false,
            Event::Window {
                win_event: WindowEvent::SizeChanged(w, h),
                ..
            } if w > 0 && h > 0 => {
                camera.width = w as u32;
                camera.height = h as u32;
                self.resizes += 1;
            }
            Event::MouseWheel {
                y,
                mouse_x,
                mouse_y,
                direction,
                ..
            } => {
                let dy = if direction == MouseWheelDirection::Flipped {
                    -y
                } else {
                    y
                };
                camera.zoom_at((dy as f64 * 0.15).exp(), [mouse_x as f64, mouse_y as f64]);
                self.zooms += 1;
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => self.drag = Some((x, y, false)),
            Event::MouseMotion {
                x, y, xrel, yrel, ..
            } => {
                if let Some((sx, sy, moved)) = &mut self.drag {
                    if (x - *sx).abs() + (y - *sy).abs() > crate::input_events::slop(5) {
                        *moved = true;
                    }
                    if *moved {
                        camera.pan(xrel as f64, yrel as f64);
                        self.pans += 1;
                    }
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if self.drag.take().is_some_and(|(_, _, m)| !m)
                    && y >= 40
                    && y < camera.height as i32 - warzone_app_client::hud::bottom_reserved(scene)
                {
                    self.selected = scene.pick(camera, [x as f64, y as f64]);
                    self.picks += 1;
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => self.selected = None,
            Event::KeyDown {
                keycode: Some(Keycode::Home),
                ..
            } => {
                camera.x = scene.camera[0];
                camera.y = scene.camera[1];
                camera.zoom = scene.camera[2] * warzone_app_client::browser::zoom_factor();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Left),
                ..
            } => camera.pan(40., 0.),
            Event::KeyDown {
                keycode: Some(Keycode::Right),
                ..
            } => camera.pan(-40., 0.),
            Event::KeyDown {
                keycode: Some(Keycode::Up),
                ..
            } => camera.pan(0., 40.),
            Event::KeyDown {
                keycode: Some(Keycode::Down),
                ..
            } => camera.pan(0., -40.),
            _ => {}
        }
        camera.constrain(scene);
        true
    }
}
fn panel_button(x: i32, y: i32, camera: &Camera) -> Option<u8> {
    if y < camera.height as i32 - 44 || y >= camera.height as i32 - 8 {
        return None;
    }
    if (16..186).contains(&x) {
        Some(0)
    } else if (200..430).contains(&x) {
        Some(1)
    } else {
        None
    }
}
#[cfg_attr(target_os = "emscripten", allow(unused_mut))]
fn run() -> Result<(), String> {
    let mut pack = None;
    let mut profile_path = None;
    let mut production_check: Option<String> = None;
    let mut systems_check: Option<String> = None;
    let mut world_check: Option<String> = None;
    let mut construction_check: Option<String> = None;
    let mut lifecycle_check: Option<String> = None;
    let mut workshop_check: Option<String> = None;
    let mut mission_script: Option<serde_json::Value> = None;
    let mut test_now: Option<i64> = None;
    let mut test_now_ms: Option<i64> = None;
    let mut test_entropy: Option<u32> = None;
    let mut frames = 0u64;
    let mut window_size = (1100, 760);
    let mut screen: Option<PathBuf> = None;
    let mut report: Option<PathBuf> = None;
    let mut software = false;
    let mut hidden = false;
    let mut check = false;
    let mut uncapped_diagnostic = false;
    let mut no_ui_cache = false;
    let mut no_ground_cache = false;
    let mut args = std::env::args().skip(1);
    #[cfg(not(target_os = "emscripten"))]
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pack" => pack = Some(PathBuf::from(args.next().ok_or("Missing pack directory")?)),
            "--profile" => {
                profile_path = Some(PathBuf::from(
                    args.next().ok_or("Missing native profile directory")?,
                ))
            }
            "--production-check" => {
                production_check = Some(args.next().ok_or("Missing production check kind")?)
            }
            "--systems-check" => systems_check = Some(args.next().ok_or("Missing systems check")?),
            "--world-check" => world_check = Some(args.next().ok_or("Missing world check")?),
            "--construction-check" => {
                construction_check = Some(args.next().ok_or("Missing construction check")?)
            }
            "--lifecycle-check" => {
                lifecycle_check = Some(args.next().ok_or("Missing lifecycle check")?);
            }
            "--workshop-check" => {
                workshop_check = Some(args.next().ok_or("Missing workshop check")?);
            }
            "--mission-script" => {
                let path = args.next().ok_or("Missing mission script path")?;
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                if bytes.len() > 1024 * 1024 {
                    return Err("Mission script limit".into());
                }
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if value["format"] != "warzone-desktop-mission-script-v1"
                    || value["steps"].as_array().is_none_or(|a| a.len() > 2048)
                {
                    return Err("Invalid mission script".into());
                }
                mission_script = Some(value);
            }
            "--test-now-ms" => {
                test_now_ms = Some(
                    args.next()
                        .ok_or("Missing test millisecond timestamp")?
                        .parse::<i64>()
                        .map_err(|_| "Invalid test millisecond time")?,
                );
            }
            "--test-now" => {
                test_now = Some(
                    args.next()
                        .ok_or("Missing test timestamp")?
                        .parse::<i64>()
                        .map_err(|_| "Invalid test time")?,
                )
            }
            "--test-entropy" => {
                test_entropy = Some(
                    args.next()
                        .ok_or("Missing entropy word")?
                        .parse::<u32>()
                        .map_err(|_| "Invalid entropy word")?,
                )
            }
            "--frames" => {
                frames = args
                    .next()
                    .ok_or("Missing frame count")?
                    .parse()
                    .map_err(|_| "Invalid frame count")?
            }
            "--window-size" => {
                let size = args.next().ok_or("Missing window size")?;
                let (w, h) = size.split_once('x').ok_or("Use WIDTHxHEIGHT")?;
                let w = w.parse::<u32>().map_err(|_| "Invalid window width")?;
                let h = h.parse::<u32>().map_err(|_| "Invalid window height")?;
                let minimum = if cfg!(target_os = "emscripten") {
                    (1, 1)
                } else {
                    (640, 480)
                };
                if !(minimum.0..=3840).contains(&w) || !(minimum.1..=2160).contains(&h) {
                    return Err("Window size outside supported bounds".into());
                }
                window_size = (w, h);
            }
            "--screenshot" => {
                screen = Some(PathBuf::from(args.next().ok_or("Missing screenshot path")?))
            }
            "--report" => report = Some(PathBuf::from(args.next().ok_or("Missing report path")?)),
            "--software" => software = true,
            "--hidden" => hidden = true,
            "--uncapped-diagnostic" => uncapped_diagnostic = true,
            "--no-ui-cache" => no_ui_cache = true,
            "--no-ground-cache" => no_ground_cache = true,
            "--self-check" => check = true,
            "--help" => {
                println!(
                    "warzone-app-client --pack DIR --profile DIR [--frames N] [--screenshot PNG] [--report JSON] [--self-check] [--software] [--hidden]"
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    // Browser builds expose the real game only. Keeping replay/report switches
    // constant removes the large native diagnostic branch from the hot loop,
    // which otherwise exhausts memory in Chromium's optimizing WASM compiler.
    #[cfg(target_os = "emscripten")]
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pack" => pack = Some(PathBuf::from(args.next().ok_or("Missing pack")?)),
            "--profile" => {
                profile_path = Some(PathBuf::from(args.next().ok_or("Missing profile")?))
            }
            "--window-size" => {
                let size = args.next().ok_or("Missing window size")?;
                let (w, h) = size.split_once('x').ok_or("Invalid window size")?;
                let w = w.parse::<u32>().map_err(|_| "Invalid width")?;
                let h = h.parse::<u32>().map_err(|_| "Invalid height")?;
                let minimum = if cfg!(target_os = "emscripten") {
                    (1, 1)
                } else {
                    (640, 480)
                };
                if !(minimum.0..=3840).contains(&w) || !(minimum.1..=2160).contains(&h) {
                    return Err("Window size outside supported bounds".into());
                }
                window_size = (w, h);
            }
            _ => return Err(format!("Unsupported browser option {arg}")),
        }
    }
    if check && frames < 90 {
        return Err("Self-check needs --frames 90 or more".into());
    }
    let pack = pack.ok_or("Use --pack to select a standalone scene package")?;
    let mut scene = Scene::load(&pack)?;
    let diagnostic = production_check.is_some()
        || systems_check.is_some()
        || world_check.is_some()
        || construction_check.is_some()
        || lifecycle_check.is_some()
        || workshop_check.is_some()
        || mission_script.is_some();
    if (no_ui_cache || no_ground_cache) && !diagnostic {
        return Err("Render cache comparison requires a separate diagnostic profile".into());
    }
    if uncapped_diagnostic && (!diagnostic || !(60..=2000).contains(&frames)) {
        return Err(
            "Uncapped timing requires separate diagnostic profile and60..2000frames".into(),
        );
    }
    if mission_script.is_some() && (!(20..=4096).contains(&frames) || scene.combat.is_none()) {
        return Err("Mission replay requires combat pack and20..4096frames".into());
    }
    if workshop_check.is_some()
        && (frames < 60
            || scene.workshop.is_none()
            || !matches!(
                workshop_check.as_deref(),
                Some(
                    "train-start"
                        | "train-wait"
                        | "train-cancel"
                        | "train-place"
                        | "lab-buy"
                        | "lab-finish"
                        | "craft-start"
                        | "craft-wait"
                        | "craft-collect"
                        | "steel-start"
                        | "steel-collect"
                        | "research-start"
                        | "research-wait"
                        | "research-collect"
                        | "train-commando"
                        | "view"
                )
            ))
    {
        return Err("Invalid workshop check or frame count".into());
    }
    if lifecycle_check.is_some()
        && (frames < 60
            || scene.lifecycle.is_none()
            || !matches!(
                lifecycle_check.as_deref(),
                Some("sell" | "cancel" | "repair" | "view")
            ))
    {
        return Err(
            "Lifecycle check needs --frames>=60, lifecycle pack, sell|cancel|repair|view".into(),
        );
    }
    if construction_check.is_some()
        && (frames < 80
            || scene.construction.is_none()
            || !matches!(
                construction_check.as_deref(),
                Some("place" | "cancel" | "wait" | "half" | "finish" | "view" | "produce")
            ))
    {
        return Err("Construction check needs --frames>=80, construction pack, place|cancel|wait|half|finish|view|produce".into());
    }
    if world_check.is_some()
        && (frames < 60
            || scene.world.is_none()
            || !matches!(
                world_check.as_deref(),
                Some("move" | "rotate" | "cancel" | "view")
            ))
    {
        return Err(
            "World check needs --frames >=60, world pack, and move|rotate|cancel|view".into(),
        );
    }
    if (test_now.is_some() || test_now_ms.is_some() || test_entropy.is_some()) && !diagnostic {
        return Err(
            "Test clock/entropy require --production-check and a separate diagnostic profile"
                .into(),
        );
    }
    if production_check.is_some()
        && (frames < 40
            || !matches!(
                production_check.as_deref(),
                Some("start" | "wait" | "collect" | "idle")
            ))
    {
        return Err("Production check requires --frames >=40 and start|wait|collect|idle".into());
    }
    if systems_check.is_some()
        && (frames < 60
            || !matches!(
                systems_check.as_deref(),
                Some("claim" | "view" | "shop" | "inventory" | "unlock")
            ))
    {
        return Err(
            "Systems check needs --frames >=60 and claim|view|shop|inventory|unlock".into(),
        );
    }
    let profile_path = profile_path.ok_or("Use --profile for a separate native save directory")?;
    let pack_dir = pack.canonicalize().map_err(|e| e.to_string())?;
    // Assets and the copied source profile are immutable. The launcher also
    // guards its source-save path; the binary rejects any existing entity archive.
    if profile_path.join("Documents/Entities2.dat").exists() {
        return Err("Cannot use an original application save as a native profile".into());
    }
    std::fs::create_dir_all(&profile_path).map_err(|e| e.to_string())?;
    if profile_path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(pack_dir)
    {
        return Err("Native profile must be outside the asset pack".into());
    }
    let seed = save::digest(&std::fs::read(pack.join("scene.json")).map_err(|e| e.to_string())?);
    let mut profile = Profile::open(&profile_path, &scene, &seed, diagnostic)?;
    scene.select_language(profile.state.language_id())?;
    let mut world = scene.clone();
    world.entities = profile.state.world_entities(&scene)?;
    if test_now.is_some() && test_now_ms.is_some() {
        return Err("Choose one diagnostic clock".into());
    }
    let fixed_ms = match test_now {
        Some(t) => Some(t.checked_mul(1000).ok_or("Invalid test time")?),
        None => test_now_ms,
    };
    let wall_ms = || {
        fixed_ms.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        })
    };
    let wall = || wall_ms() / 1000;
    if profile.state.mission.is_none()
        && scene
            .combat
            .as_ref()
            .is_some_and(|c| c.tutorial_steps.len() == 27)
        && profile.state.systems.as_ref().is_some_and(|p| {
            !p.flag("TutorialComplete") && !p.flag("tutorialCompleted") && !p.flag("Mission-5000")
        })
    {
        let (next, _) = profile.state.mission_apply(
            &scene,
            &warzone_app_client::mission::Command::Open("tutorial_mission".into()),
            wall(),
        )?;
        profile.commit(&scene, next)?;
    }
    if systems_check.as_deref() == Some("claim") {
        if !profile.state.actions.is_empty() {
            return Err("Systems claim check requires a NEW diagnostic profile".into());
        }
        let mut fixture = profile.state.clone();
        fixture.resources.insert("exp".into(), 570);
        fixture.sync_preferences();
        profile.commit(&scene, fixture)?;
    }
    #[cfg(not(target_os = "emscripten"))]
    // SAFETY: Startup is still single-threaded, before SDL/video/audio workers
    // are initialized. SDL2 reads these values during video initialization.
    unsafe {
        std::env::set_var("SDL_VIDEO_X11_WMCLASS", "warzone-client");
        std::env::set_var("SDL_VIDEO_WAYLAND_WMCLASS", "warzone-client");
    }
    let sdl = sdl2::init()?;
    #[cfg(target_os = "emscripten")]
    sdl2::hint::set("SDL_EMSCRIPTEN_ASYNCIFY", "0"); // one explicit rAF yield per frame
    let video = sdl.video()?;
    let mut wb = video.window("Warzone", window_size.0, window_size.1);
    // SDL's Emscripten resizable-window path replaces the backing buffer with
    // getBoundingClientRect(), losing Retina pixels and swapping axes for the
    // portrait fallback. The web shell owns canvas sizing; native windows keep
    // the usual OS resize behaviour.
    #[cfg(not(target_os = "emscripten"))]
    wb.resizable();
    wb.position_centered();
    if hidden {
        wb.hidden();
    }
    let mut window = wb.build().map_err(|e| e.to_string())?;
    window
        .set_minimum_size(640, 480)
        .map_err(|e| e.to_string())?;
    let builder = window.into_canvas();
    let mut canvas = if software {
        // An explicit driver index wins over an inherited SDL_RENDER_DRIVER.
        let index = sdl2::render::drivers()
            .position(|d| d.name == "software")
            .ok_or("SDL software renderer is unavailable")?;
        builder.index(index as u32).software().build()
    } else {
        if uncapped_diagnostic {
            builder.accelerated().build()
        } else {
            builder.accelerated().present_vsync().build()
        }
    }
    .map_err(|e| e.to_string())?;
    let vsync_active = !software
        && !uncapped_diagnostic
        && canvas.info().flags & sdl2::sys::SDL_RendererFlags::SDL_RENDERER_PRESENTVSYNC as u32
            != 0;
    println!(
        "Native renderer: {}; entities: {}; terrain sprites: {}",
        canvas.info().name,
        world.entities.len(),
        scene.terrain.len()
    );
    if let Ok(raw) = std::fs::read(pack.join("app-icon.png")) {
        let decoder = png::Decoder::new(std::io::Cursor::new(raw));
        if let Ok(mut reader) = decoder.read_info() {
            if let Some(size) = reader.output_buffer_size() {
                let mut pixels = vec![0; size];
                if let Ok(info) = reader.next_frame(&mut pixels) {
                    let rgba = match info.color_type {
                        png::ColorType::Rgba => pixels[..info.buffer_size()].to_vec(),
                        png::ColorType::Rgb => pixels[..info.buffer_size()]
                            .chunks_exact(3)
                            .flat_map(|p| [p[0], p[1], p[2], 255])
                            .collect(),
                        _ => vec![],
                    };
                    if !rgba.is_empty() {
                        let mut rgba = rgba;
                        if let Ok(surface) = sdl2::surface::Surface::from_data(
                            &mut rgba,
                            info.width,
                            info.height,
                            info.width * 4,
                            sdl2::pixels::PixelFormatEnum::RGBA32,
                        ) {
                            canvas.window_mut().set_icon(surface);
                        };
                    }
                }
            }
        }
    }
    let creator = canvas.texture_creator();
    let mut art = render::Art::load(&creator, &scene, &pack)?;
    art.trace_source_buttons = report.is_some();
    art.cache_source_views = !software && !no_ui_cache;
    art.cache_ground = !no_ground_cache;
    let mut audio_error = None;
    let mut audio = match audio::Audio::open(&sdl, &scene, &pack) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Звук недоступен: {e}");
            audio_error = Some(e);
            None
        }
    };
    if let Some(a) = &mut audio {
        a.sync(&profile.state);
    }
    warzone_app_client::browser::update_display_density();
    let mut zoom_factor = warzone_app_client::browser::zoom_factor();
    let (w, h) = canvas.output_size()?;
    let mut camera = Camera {
        x: profile.state.camera[0],
        y: profile.state.camera[1],
        zoom: profile.state.camera[2] * zoom_factor,
        width: w,
        height: h,
    };
    #[cfg(target_os = "emscripten")]
    art.warm_menus(&mut canvas, &scene, &profile.state, &camera)?;
    let mut input = Input {
        selected: profile
            .state
            .selected
            .as_ref()
            .and_then(|uid| world.entities.iter().position(|e| &e.uid == uid)),
        ..Input::default()
    };
    let mut view_revision = u64::MAX;
    let mut view_time = 0;
    let mut recipe_index = 0usize;
    let mut ui = ui::Ui::default();
    ui.initialize_level(&profile.state);
    ui.enable_bottom_motion();
    ui.initialize_receipts(profile.state.revision);
    let mut tactical = if profile.state.mission.is_some() && !profile.state.mission_paused() {
        Some(tactical::Ui::new(&scene, &profile.state, w, h)?)
    } else {
        None
    };
    let mut editor = editor::Editor::default();
    let mut message = if profile.state.mission.is_some() && !profile.state.mission_paused() {
        "Бой восстановлен · Q — цели · Закрытие окна сохраняет бой"
    } else {
        "H — харвестер · U — казарма · N — исследования · F5 — миссия"
    }
    .to_string();
    let mut action_results = Vec::new();
    let mut mission_action_trace = Vec::new();
    let mut script_camera_wait_frames = 0;
    let mut layout_results = Vec::new();
    let mut layout_previews = Vec::new();
    let mut events = sdl.event_pump()?;
    let push = sdl.event()?;
    #[cfg(target_os = "emscripten")]
    let mut saved_pose = (profile.state.camera, input.selected);
    #[cfg(target_os = "emscripten")]
    let mut observed_pose = saved_pose;
    #[cfg(target_os = "emscripten")]
    let mut pose_changed = Instant::now();
    let started = Instant::now();
    let mut period = Instant::now();
    let mut last_frames = 0;
    let mut count = 0;
    let mut draws = 0;
    let mut fps_samples = Vec::new();
    let mut frame_metrics = frame_metrics::FrameMetrics::default();
    let mut screenshots = Vec::new();
    let mut motion_samples = Vec::new();
    let mut quest_camera_samples = Vec::new();
    let mut loading_frames = 0u64;
    'app: loop {
        // Canvas resizing/fullscreen need not produce SDL SizeChanged on the
        // web. Both base and battle must use the actual drawing-buffer size
        // before building hit targets or consuming the next pointer event.
        warzone_app_client::browser::update_display_density();
        let next_zoom_factor = warzone_app_client::browser::zoom_factor();
        if next_zoom_factor != zoom_factor {
            camera.zoom *= next_zoom_factor / zoom_factor;
            if let Some(controller) = &mut tactical {
                controller.camera.zoom *= next_zoom_factor / zoom_factor;
            }
            zoom_factor = next_zoom_factor;
        }
        #[cfg(target_os = "emscripten")]
        let (output_width, output_height) = warzone_app_client::browser::drawing_size();
        #[cfg(not(target_os = "emscripten"))]
        let (output_width, output_height) = canvas.output_size()?;
        #[cfg(target_os = "emscripten")]
        if canvas.window().size() != (output_width, output_height) {
            canvas
                .window_mut()
                .set_size(output_width, output_height)
                .map_err(|e| e.to_string())?;
        }
        camera.width = output_width;
        camera.height = output_height;
        if let Some(controller) = &mut tactical {
            controller.camera.width = output_width;
            controller.camera.height = output_height;
        }
        let pointer_cancelled = warzone_app_client::browser::take_pointer_cancel();
        if pointer_cancelled {
            input.drag = None;
            ui.cancel_pointer();
            editor.cancel_pointer();
            if let Some(controller) = &mut tactical {
                controller.cancel_pointer(&scene, &profile.state)?;
            }
        }

        let tick = Instant::now();
        ui.animation_seconds = started.elapsed().as_secs_f64();
        if tactical.is_none()
            && ui.advance_quest_camera(&mut camera)
            && mission_script.is_some()
            && quest_camera_samples.len() < 2048
        {
            quest_camera_samples.push(serde_json::json!({"frame":count,"seconds":ui.animation_seconds,"x":camera.x,"y":camera.y,"scrolling":ui.quest_scrolling()}));
        }

        let now = profile
            .state
            .clock(scene.economy.as_ref().unwrap(), wall())?;
        if !profile.state.mission_paused()
            && profile.state.mission.as_ref().is_some_and(|m| {
                m.battle.pvp.is_some() && m.battle.result.is_none() && now - m.opened >= 2700
            })
        {
            let (next, text) = profile.state.mission_apply_with_entropy(
                &scene,
                &warzone_app_client::mission::Command::PvpTimeout,
                wall(),
                || test_entropy.map(Ok).unwrap_or_else(save::entropy),
            )?;
            profile.commit(&scene, next)?;
            message = text;
        }
        if let Some(controller) = &mut tactical {
            controller.update_clock(&scene, &profile.state, now);
        }
        if profile.state.obstacles_due(&scene, wall())? {
            if let Some(next) = profile.state.settle_obstacles(&scene, wall(), || {
                test_entropy.map(Ok).unwrap_or_else(save::entropy)
            })? {
                profile.commit(&scene, next)?;
            }
        }
        if profile.state.upgrade_gathers_due_ms(&scene, wall_ms())? {
            if let Some(next) = profile.state.settle_upgrade_gathers_ms(&scene, wall_ms())? {
                profile.commit(&scene, next)?;
            }
        }
        if profile.state.production_gathers_due_ms(&scene, wall_ms())? {
            if let Some(next) =
                profile
                    .state
                    .settle_production_gathers_ms(&scene, wall_ms(), || {
                        test_entropy.map(Ok).unwrap_or_else(save::entropy)
                    })?
            {
                profile.commit(&scene, next)?;
            }
        }
        if profile.state.upgrade_flights_due_ms(&scene, wall_ms())? {
            if let Some(next) = profile.state.settle_upgrade_flights_ms(&scene, wall_ms())? {
                profile.commit(&scene, next)?;
            }
        }
        if profile.state.research_flights_due_ms(&scene, wall_ms())? {
            if let Some(next) = profile
                .state
                .settle_research_flights_ms(&scene, wall_ms())?
            {
                profile.commit(&scene, next)?;
            }
        }
        if tactical.is_none() && (profile.state.revision != view_revision || now != view_time) {
            let selected = input
                .selected
                .and_then(|i| world.entities.get(i))
                .map(|e| e.uid.clone());
            world.entities = profile.state.world_entities_at(&scene, Some(now))?;
            input.selected =
                selected.and_then(|uid| world.entities.iter().position(|e| e.uid == uid));
            view_revision = profile.state.revision;
            view_time = now;
        }

        if let Some(script) = &mission_script {
            // Admit pending presentation/camera/story state before consulting
            // input readiness. A motion can finish between rendered frames;
            // otherwise its first story key sees a stale empty animation group
            // and is consumed by the newly initialized source text lock.
            if let Some(controller) = &mut tactical {
                controller.sync(&scene, &profile.state)?;
            } else {
                ui.update_time_ms(
                    &scene,
                    &profile.state,
                    profile.state.callback_game_ms(&scene, wall_ms())?,
                );
            }
            // Real pointer input is locked while an original scripted camera
            // pan runs. A fixed-frame diagnostic must wait for that same UI,
            // not silently drop a paid shot and execute the remaining trace
            // against a different battle. The ordinary game clock, animation
            // and all source rules keep running; only the test-script cursor
            // is delayed, bounded by the4096frame diagnostic limit.
            ui.preview_drag_geometry =
                editor.drag_geometry(&scene, &world, &profile.state, &camera);
            let script_frame = count - script_camera_wait_frames;
            let waiting = script["steps"].as_array().is_some_and(|steps| {
                steps.iter().any(|step| {
                    if step["frame"].as_u64() != Some(script_frame) {
                        return false;
                    }
                    let action = &step["action"];
                    let pointer = action[0].as_str().is_some_and(|kind| kind != "key");
                    let advance = action[0] == "key"
                        && (action[1] == "Enter" || action[1] == "Space")
                        || pointer && (action[0] == "battle_ui" || action[0] == "ui");
                    tactical.as_ref().map_or_else(
                        || !ui.bottom_input_ready() || advance && !ui.story_input_ready(),
                        |u| {
                            !u.result_presentation_ready()
                                || pointer
                                    && (u.script_pointer_blocked(&scene, &profile.state)
                                        || u.script_bonus_obstructs(&scene, &profile.state, action))
                                || advance && !u.story_input_ready(&scene, &profile.state)
                        },
                    )
                })
            });
            if waiting {
                script_camera_wait_frames += 1;
            } else {
                tactical::script(
                    script_frame,
                    script,
                    &push,
                    canvas.window().id(),
                    tactical.as_ref(),
                    &scene,
                    &camera,
                    &profile.state,
                    &ui,
                )
                .map_err(|e| {
                    format!(
                        "Diagnostic script frame {script_frame}: {e}; recent commands={:?}",
                        mission_action_trace
                            .iter()
                            .rev()
                            .take(3)
                            .collect::<Vec<_>>()
                    )
                })?;
            }
        }
        if let Some(mode) = world_check.as_deref() {
            let grab = editor.drag_geometry(&scene, &world, &profile.state, &camera);
            layout_script(
                count,
                mode,
                &push,
                canvas.window().id(),
                &mut camera,
                &world,
                grab,
            )?;
        }
        if let Some(mode) = construction_check.as_deref() {
            construction_script(
                count,
                mode,
                &push,
                canvas.window().id(),
                &mut camera,
                &world,
                &scene,
                &profile.state,
            )?;
        }
        if let Some(mode) = lifecycle_check.as_deref() {
            lifecycle_script(
                count,
                mode,
                &push,
                canvas.window().id(),
                &mut camera,
                &world,
                &ui,
                &profile.state,
            )?;
        }
        if let Some(mode) = workshop_check.as_deref() {
            workshop_script(
                count,
                mode,
                &push,
                canvas.window().id(),
                &mut camera,
                &world,
                &scene,
                &profile.state,
            )?;
        }
        if check {
            if count == 40 {
                canvas
                    .window_mut()
                    .set_size(960, 680)
                    .map_err(|e| e.to_string())?;
            }
            script(count, &push, canvas.window().id(), &camera, &world)?;
        }
        if let Some(mode) = systems_check.as_deref() {
            if count == 5 && matches!(mode, "claim" | "unlock") {
                let type_id = if mode == "claim" { 1010 } else { 1007 };
                let i = world
                    .entities
                    .iter()
                    .position(|e| e.type_id == type_id)
                    .ok_or("Missing diagnostic building")?;
                input.selected = Some(i);
                let e = &world.entities[i];
                let p = scene.projection.project(e.x, e.y);
                camera.x = p[0];
                camera.y = p[1];
                if mode == "unlock" {
                    let c = scene.economy.as_ref().unwrap();
                    recipe_index = c.buildings[&type_id]
                        .recipes
                        .iter()
                        .position(|id| profile.state.needs_recipe_unlock(&scene, *id))
                        .ok_or("No locked recipe available")?;
                }
            }
            let click_frame = if mode == "claim" { 30 } else { 15 };
            if matches!(mode, "claim" | "shop")
                && (count == click_frame || count == click_frame + 1)
            {
                let r = render::modal_layout(&camera).claim;
                let x = r.x() + 30;
                let y = r.y() + 12;
                let event = if count == click_frame {
                    Event::MouseButtonDown {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    }
                } else {
                    Event::MouseButtonUp {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    }
                };
                push.push_event(event)?;
            }
            let key = match mode {
                "claim" => {
                    if count == 20 {
                        ui.index = profile
                            .state
                            .active_quests(&scene)
                            .iter()
                            .position(|q| q.id == 4)
                            .ok_or("Quest4 missing")?;
                    }
                    match count {
                        10 => Some(Keycode::Return),
                        20 => Some(Keycode::Q),
                        _ => None,
                    }
                }
                "unlock" => {
                    if count == 10 || count == 20 {
                        Some(Keycode::Return)
                    } else {
                        None
                    }
                }
                "shop" => {
                    if count == 5 {
                        ui.index = profile
                            .state
                            .shop_items(&scene)
                            .iter()
                            .position(|i| i.id == 4001)
                            .ok_or("Pack4001 missing")?;
                        Some(Keycode::B)
                    } else {
                        None
                    }
                }
                "inventory" => {
                    if count == 5 {
                        Some(Keycode::I)
                    } else {
                        None
                    }
                }
                _ => {
                    if count == 5 {
                        Some(Keycode::Q)
                    } else {
                        None
                    }
                }
            };
            if let Some(keycode) = key {
                push.push_event(Event::KeyDown {
                    timestamp: 0,
                    window_id: canvas.window().id(),
                    keycode: Some(keycode),
                    scancode: None,
                    keymod: Mod::NOMOD,
                    repeat: false,
                })?;
            }
        }
        if production_check.is_some() {
            if count == 5 {
                let i = world
                    .entities
                    .iter()
                    .position(|e| e.type_id == 1027)
                    .ok_or("Production check needs harvester")?;
                input.selected = Some(i);
                let e = &world.entities[i];
                let p = scene.projection.project(e.x, e.y);
                camera.x = p[0];
                camera.y = p[1];
            }
            if production_check.as_deref() == Some("collect") && (count == 10 || count == 11) {
                let (x, y) = (70, camera.height as i32 - 25);
                let event = if count == 10 {
                    Event::MouseButtonDown {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    }
                } else {
                    Event::MouseButtonUp {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    }
                };
                push.push_event(event)?;
            }
            if count == 10 && !matches!(production_check.as_deref(), Some("idle" | "collect")) {
                // Real SDL event goes through the same control and transaction
                // as a user's Enter key. Test profiles cannot open as real games.
                push.push_event(Event::KeyDown {
                    timestamp: 0,
                    window_id: canvas.window().id(),
                    keycode: Some(Keycode::Return),
                    scancode: None,
                    keymod: Mod::NOMOD,
                    repeat: false,
                })?;
            }
        }
        let mut training_opened_this_frame = false;
        let frame_events = input_events::coalesce(events.poll_iter(), pointer_cancelled);
        #[cfg(target_os = "emscripten")]
        let frame_events = {
            let window = canvas.window().size();
            let output = canvas.output_size()?;
            warzone_app_client::browser::viewport(window, output);
            frame_events
                .into_iter()
                .map(|e| input_events::for_output(e, window, output))
                .collect::<Vec<_>>()
        };
        for event in frame_events {
            if training_opened_this_frame
                && matches!(
                    event,
                    Event::MouseButtonDown { .. } | Event::MouseButtonUp { .. }
                )
            {
                continue;
            }
            if let Event::KeyDown {
                keycode: Some(k @ (Keycode::F6 | Keycode::F7)),
                repeat: false,
                ..
            } = event
            {
                let (next, text) = profile.state.toggle_audio(
                    &scene,
                    if k == Keycode::F6 {
                        "playMusic"
                    } else {
                        "playSound"
                    },
                )?;
                profile.commit(&scene, next)?;
                message = text;
                if let Some(a) = &mut audio {
                    a.sync(&profile.state);
                }
                continue;
            }
            if let Some(controller) = &mut tactical {
                if matches!(event, Event::Quit { .. }) {
                    break 'app;
                }
                if let Event::Window {
                    win_event: WindowEvent::SizeChanged(w, h),
                    ..
                } = event
                {
                    if w > 0 && h > 0 {
                        camera.width = w as u32;
                        camera.height = h as u32;
                    }
                }
                if let Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } = event
                {
                    if controller
                        .source_view(&scene, &profile.state)
                        .and_then(|v| v.hit(x, y))
                        .is_some_and(|a| {
                            !matches!(
                                a,
                                warzone_app_client::interface::Action::FallingPickup(..)
                                    | warzone_app_client::interface::Action::Choose(_)
                            )
                        })
                    {
                        if let Some(a) = &mut audio {
                            a.event("ui/button_click");
                        }
                    }
                }
                let previous_selection = controller.selected_type();
                let command = controller.event(event, &scene, &profile.state)?;
                if previous_selection != controller.selected_type()
                    && profile
                        .state
                        .mission
                        .as_ref()
                        .is_some_and(|m| m.battle.phase == "battle")
                {
                    if let (Some(a), Some(id)) = (&mut audio, controller.selected_type()) {
                        a.unit_event(id, "click");
                    }
                }
                if let Some(command) = command {
                    let command_started = Instant::now();
                    match profile
                        .state
                        .mission_apply_with_entropy(&scene, &command, wall(), || {
                            test_entropy.map(Ok).unwrap_or_else(save::entropy)
                        }) {
                        Ok((mut next, text)) => {
                            let simulation_ms = command_started.elapsed().as_secs_f64() * 1000.;
                            controller.save_camera(&mut next);
                            let commit_started = Instant::now();
                            profile.commit(&scene, next)?;
                            if mission_script.is_some() && mission_action_trace.len() < 4096 {
                                mission_action_trace.push(serde_json::json!({"frame":count,"command":format!("{command:?}"),"simulation_ms":simulation_ms,"commit_ms":commit_started.elapsed().as_secs_f64()*1000.,"energy":profile.state.mission.as_ref().map(|m|m.battle.energy),"pvp_message":warzone_app_client::story::current(&scene,&profile.state).map(|p|p.0)}));
                            }
                            if matches!(command, warzone_app_client::mission::Command::Place { .. })
                            {
                                if let Some(a) = &mut audio {
                                    a.event("ui/unit_put");
                                }
                            }
                            message = text;
                            action_results.push(true);
                            // Show the source loader before preparing the base
                            // objects/textures; cached returns need no fake delay.
                            if profile.state.mission.is_none()
                                && art.loading(
                                    &mut canvas,
                                    &scene,
                                    &camera,
                                    profile.state.language_id(),
                                )?
                            {
                                canvas.present();
                                loading_frames += 1;
                            }
                            if profile.state.mission.is_none()
                                || matches!(command, warzone_app_client::mission::Command::Suspend)
                            {
                                tactical = None;
                                if profile.state.mission.is_none() {
                                    ui.kind = None;
                                    ui.selection = None;
                                    input.selected = None;
                                }
                                input.drag = None;
                                view_revision = u64::MAX;
                            }
                        }
                        Err(text) => {
                            if text == "Недостаточно энергии"
                                && controller.show_energy_refill(&scene, &profile.state)
                            {
                                // Reject the whole attempted turn; show the source refill dialog.
                            } else if warzone_app_client::interface_battle::available(&scene)
                                && profile
                                    .state
                                    .mission
                                    .as_ref()
                                    .is_none_or(|s| s.battle.tutorial.is_none())
                            {
                                controller.notice = Some(text.clone());
                            }
                            message = text;
                            action_results.push(false);
                        }
                    }
                }
                continue;
            }
            let mut activate = false;
            ui.selection = input
                .selected
                .and_then(|i| world.entities.get(i))
                .map(|e| e.uid.clone());
            ui.editing = editor.edit.is_some();
            ui.placement_only = editor.training.is_some();
            ui.update_time_ms(
                &scene,
                &profile.state,
                profile.state.callback_game_ms(&scene, wall_ms())?,
            );
            for _ in 0..ui.take_quest_completion_sounds() {
                if let Some(a) = &mut audio {
                    a.event("ui/quest_dialog");
                }
            }
            if let Some(uid) = ui.update_guide_focus(&scene, &profile.state, &camera) {
                input.selected = world.entities.iter().position(|e| e.uid == uid);
            }
            ui.prepare_reward_camera(&scene, &camera);
            let previous_panel = ui.kind;
            // A gesture begun on the map owns its motion/release. Otherwise
            // crossing the HUD cancels the camera drag and can open a quest
            // on release, even though the press never belonged to that button.
            let map_gesture = !ui.owns_native_pointer()
                && !ui.guide_map_locked()
                && (input.drag.is_some() || editor.owns_pointer())
                && matches!(
                    event,
                    Event::MouseMotion { .. }
                        | Event::MouseButtonUp {
                            mouse_btn: MouseButton::Left,
                            ..
                        }
                );
            let mut ui_action = if map_gesture {
                ui::EventResult::Pass
            } else {
                ui.event(&event, &camera, &scene, &profile.state)
            };
            #[cfg(target_os = "emscripten")]
            warzone_app_client::browser::trace_pointer(
                &event,
                !matches!(ui_action, ui::EventResult::Pass),
                ui.kind.map_or(-1, |k| k as i32),
            );
            for _ in 0..ui.take_button_taps() {
                if let Some(a) = &mut audio {
                    a.event("ui/button_click");
                }
            }
            let mut map_input_applied = false;
            if matches!(ui_action, ui::EventResult::Pass)
                && editor.edit.is_none()
                && matches!(
                    event,
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        ..
                    }
                )
            {
                let previous_selected = input.selected;
                let picks = input.picks;
                if !input.event(event.clone(), &mut camera, &world) {
                    break 'app;
                }
                map_input_applied = true;
                if input.picks != picks {
                    if input.selected.is_none() {
                        if let Event::MouseButtonUp { x, y, .. } = event {
                            if let Some(id) =
                                profile
                                    .state
                                    .expansion_at(&scene, &camera, [x as f64, y as f64])
                            {
                                ui_action = ui.native_action(
                                    warzone_app_client::interface::Action::LandOpen(id),
                                    &scene,
                                    &profile.state,
                                );
                            }
                        }
                    }
                    if let Some(e) = input.selected.and_then(|i| world.entities.get(i)) {
                        let uid = &e.uid;
                        if profile.state.gathering(uid) {
                            // SceneMain ignores touches on bBuildingBusy.
                            input.selected = previous_selected;
                            continue;
                        }
                        ui.selection = Some(uid.clone());
                        if let Some(a) = &mut audio {
                            a.unit_event(e.type_id, "click");
                        }
                        if !profile.state.repair_pending(&scene, uid)
                            && warzone_app_client::world_markers::is_ready(
                                &scene,
                                &profile.state,
                                uid,
                                now,
                            )
                        {
                            ui_action = ui::EventResult::ObjectPrimary(uid.clone());
                        } else if ui.open_selected_window(
                            &scene,
                            &profile.state,
                            uid,
                            previous_selected != input.selected,
                        ) {
                            ui_action = ui::EventResult::Consumed;
                        }
                    }
                }
            }
            if let ui::EventResult::ObjectPrimary(uid) = &ui_action {
                if let Some(prompt) = profile.state.collection_capacity_prompt(&scene, uid, now) {
                    ui.offer_capacity(prompt);
                    input.selected = None;
                    input.drag = None;
                    continue;
                }
            }
            if let ui::EventResult::ObjectPrimary(uid) = ui_action {
                input.selected = world.entities.iter().position(|e| e.uid == uid);
                ui.selection = Some(uid.clone());
                // Execute the captured collect command directly. Queuing a
                // generic Enter lets a second click start a fresh contract.
                ui_action = if profile.state.repair_pending(&scene, &uid) {
                    ui::EventResult::ObjectRepair(uid)
                } else if profile.state.construction_pending(&scene, &uid) {
                    ui::EventResult::ObjectFinish(uid)
                } else if let Some(cmd @ warzone_app_client::production::Command::Collect { .. }) =
                    profile.state.upgrade_command(&uid, 0)
                {
                    ui::EventResult::UpgradeApply(cmd)
                } else if let Some(cmd @ warzone_app_client::production::Command::Collect { .. }) =
                    profile.state.research_command(&scene, &uid, 0)
                {
                    ui::EventResult::ObjectResearchCollect(cmd)
                } else if let Some(cmd @ warzone_app_client::production::Command::Collect { .. }) =
                    profile.state.workshop_command(&scene, &uid, 0)
                {
                    if let warzone_app_client::production::Command::Collect {
                        recipe,
                        started,
                        ..
                    } = &cmd
                    {
                        if scene.workshop.as_ref().unwrap().recipes[recipe].unit() {
                            ui::EventResult::ObjectTraining(
                                warzone_app_client::production::ProductionSource {
                                    uid: uid.clone(),
                                    started: *started,
                                },
                                *recipe,
                            )
                        } else {
                            ui::EventResult::ObjectWorkshopCollect(cmd)
                        }
                    } else {
                        unreachable!()
                    }
                } else if let Some(cmd @ warzone_app_client::production::Command::Collect { .. }) =
                    profile
                        .state
                        .command(scene.economy.as_ref().unwrap(), &uid, 0)
                {
                    ui::EventResult::ObjectCollect(cmd)
                } else {
                    ui::EventResult::Consumed
                };
            }
            match ui_action {
                ui::EventResult::QuestGuide(q, i) => {
                    match ui.begin_quest_guide_animated(&scene, &profile.state, &mut camera, q, i) {
                        Ok(()) => {
                            input.selected = None;
                            input.drag = None;
                        }
                        Err(text) => ui.notice = Some(text),
                    }
                    continue;
                }
                ui::EventResult::Pass => {}
                ui::EventResult::AcademyResearch(uid, unit) => {
                    input.selected = world.entities.iter().position(|e| e.uid == uid);
                    ui.academy_research(&scene, uid, unit);
                    input.drag = None;
                    continue;
                }
                ui::EventResult::ProductionTopup(command) => {
                    match profile
                        .state
                        .start_with_shortfall(&scene, &command, wall(), || {
                            test_entropy.map(Ok).unwrap_or_else(save::entropy)
                        }) {
                        Ok((next, text)) => {
                            profile.commit(&scene, next)?;
                            message = text;
                            world.entities = profile.state.world_entities_at(&scene, Some(now))?;
                            input.selected = None;
                            ui.kind = None;
                        }
                        Err(text) => {
                            message = text.clone();
                            ui.notice = Some(text);
                        }
                    }
                    input.drag = None;
                    continue;
                }
                ui::EventResult::BuildingTopup => {
                    editor.pay_shortfall = true;
                    push.push_event(Event::KeyDown {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        keycode: Some(Keycode::Return),
                        scancode: None,
                        keymod: Mod::NOMOD,
                        repeat: false,
                    })?;
                    input.drag = None;
                    continue;
                }
                ui::EventResult::Key(k) => {
                    push.push_event(Event::KeyDown {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        keycode: Some(k),
                        scancode: None,
                        keymod: Mod::NOMOD,
                        repeat: false,
                    })?;
                    input.drag = None;
                    continue;
                }
                ui::EventResult::ObjectTraining(source, recipe) => {
                    if ui.begin_training_instruction(&profile.state, &source, recipe) {
                        let mut next = profile.state.clone();
                        next.systems
                            .as_mut()
                            .unwrap()
                            .set("FirstUnitPlacingDialog".into(), 0);
                        profile.commit(&scene, next)?;
                        input.drag = None;
                        continue;
                    }
                    match editor.plan_training(
                        &scene,
                        &profile.state,
                        &camera,
                        source,
                        recipe,
                        wall(),
                    ) {
                        Ok(()) => {
                            ui.kind = None;
                            training_opened_this_frame = true;
                            message = "Выбери место для отряда · Enter: разместить · Esc: отменить"
                                .into();
                        }
                        Err(text) => {
                            ui.notice = Some(text.clone());
                            message = text;
                        }
                    }
                    input.drag = None;
                    continue;
                }
                ui::EventResult::Production(index) => {
                    recipe_index = index;
                    push.push_event(Event::KeyDown {
                        timestamp: 0,
                        window_id: canvas.window().id(),
                        keycode: Some(Keycode::Return),
                        scancode: None,
                        keymod: Mod::NOMOD,
                        repeat: false,
                    })?;
                    input.drag = None;
                    continue;
                }
                ui::EventResult::Notice(text) => {
                    ui.notice = Some(text.clone());
                    message = text;
                    input.drag = None;
                    continue;
                }
                ui::EventResult::Consumed => {
                    if let Some(a) = &mut audio {
                        if previous_panel != ui.kind {
                            a.event(if ui.kind.is_some() {
                                "ui/window_show"
                            } else {
                                "ui/window_hide"
                            });
                        }
                    }
                    if ui.kind.is_some() {
                        editor.cancel();
                    }
                    input.drag = None;
                    continue;
                }
                ui::EventResult::SaleCancelled => {
                    input.selected = None;
                    input.drag = None;
                    editor.cancel();
                    if let Some(a) = &mut audio {
                        a.event("ui/window_hide");
                    }
                    continue;
                }
                ui::EventResult::OpenMission(name) => {
                    let command = if profile.state.mission_paused()
                        && profile
                            .state
                            .mission
                            .as_ref()
                            .is_some_and(|m| m.battle.mission == name)
                    {
                        warzone_app_client::mission::Command::Resume(name)
                    } else {
                        warzone_app_client::mission::Command::Open(name)
                    };
                    match profile
                        .state
                        .mission_apply_with_entropy(&scene, &command, wall(), || {
                            test_entropy.map(Ok).unwrap_or_else(save::entropy)
                        }) {
                        Ok((next, text)) => {
                            profile.commit(&scene, next)?;
                            tactical = Some(tactical::Ui::new(
                                &scene,
                                &profile.state,
                                camera.width,
                                camera.height,
                            )?);
                            ui.kind = None;
                            editor.cancel();
                            input.drag = None;
                            message = text;
                        }
                        Err(text) => {
                            ui.notice = Some(text.clone());
                            message = text;
                        }
                    }
                    continue;
                }
                ui::EventResult::QuestStoryOpen(_) | ui::EventResult::QuestStoryNext => {
                    let result = match ui_action {
                        ui::EventResult::QuestStoryOpen(id) => {
                            profile.state.begin_quest_story(&scene, id, wall())
                        }
                        _ => profile.state.advance_quest_story(&scene, wall()),
                    };
                    match result {
                        Ok((next, text)) => {
                            profile.commit(&scene, next)?;
                            ui.kind = Some(ui::Kind::Quests);
                            message = text;
                        }
                        Err(text) => message = text,
                    }
                    input.drag = None;
                    continue;
                }
                ui::EventResult::PlanBuilding(id) => {
                    match editor.plan_building(&scene, &profile.state, &camera, id) {
                        Ok(()) => {
                            if id == 1027 {
                                if let Some(e) = &editor.edit {
                                    let p = scene.projection.project(e.x as f64, e.y as f64);
                                    camera.x = p[0];
                                    camera.y = p[1] - camera.height as f64 * 0.1 / camera.zoom;
                                }
                            }
                            ui.kind = None;
                            message =
                                "Выбери свободное место · Enter: купить · Esc: отменить".into();
                        }
                        Err(text) => message = text,
                    }
                    input.drag = None;
                    continue;
                }
                ui::EventResult::SetLanguage(id) => {
                    match profile.state.set_language(&scene, id) {
                        Ok((next, text)) => {
                            profile.commit(&scene, next)?;
                            scene.select_language(id)?;
                            world = scene.clone();
                            world.entities = profile.state.world_entities_at(&scene, Some(now))?;
                            ui.kind = None;
                            input.selected = None;
                            input.drag = None;
                            view_revision = u64::MAX;
                            message = text;
                        }
                        Err(text) => ui.notice = Some(text),
                    }
                    continue;
                }
                action => {
                    let shop_unlocked = if let ui::EventResult::UnlockShop(id) = &action {
                        Some(*id)
                    } else {
                        None
                    };
                    let settings = matches!(&action, ui::EventResult::ToggleAudio(_));
                    let keep_panel = matches!(&action, ui::EventResult::ClaimAchievement(..))
                        || settings
                        || (scene.shop.is_some()
                            && matches!(
                                &action,
                                ui::EventResult::Buy(_)
                                    | ui::EventResult::UnlockShop(_)
                                    | ui::EventResult::BankClaim(_)
                            ))
                        || matches!(
                            &action,
                            ui::EventResult::BuyMaterial(_)
                                | ui::EventResult::BuyResearch(..)
                                | ui::EventResult::ResearchUnlock(..)
                                | ui::EventResult::RecipeUnlock(..)
                        );
                    let special_bought = if let ui::EventResult::Buy(id) = &action {
                        Some(*id)
                    } else {
                        None
                    };
                    let research_bought = matches!(&action, ui::EventResult::BuyResearch(..));
                    let upgrade_bought = matches!(&action, ui::EventResult::BuyUpgrade(..));
                    let upgrade_gather = matches!(
                        &action,
                        ui::EventResult::UpgradeApply(
                            warzone_app_client::production::Command::Collect { .. }
                        ) | ui::EventResult::ObjectResearchCollect(_)
                            | ui::EventResult::ObjectWorkshopCollect(_)
                            | ui::EventResult::ObjectCollect(_)
                    );
                    let keep_panel =
                        keep_panel || matches!(&action, ui::EventResult::BuyUpgrade(..));
                    let result = match action {
                        ui::EventResult::BonusPickup(id) => {
                            profile.state.pickup_reward_bonus_ms(&scene, id, wall_ms())
                        }
                        ui::EventResult::ClearObstacle(uid) => {
                            profile.state.clear_obstacle(&scene, &uid, wall())
                        }
                        ui::EventResult::BuyLand(id, payment) => {
                            profile.state.buy_expansion(&scene, id, payment, wall())
                        }
                        ui::EventResult::UpgradeApply(command) => profile
                            .state
                            .upgrade_ui_apply_ms(&scene, &command, wall_ms()),
                        ui::EventResult::BuyUpgrade(uid, id) => {
                            profile.state.buy_upgrade(&scene, &uid, id, wall())
                        }
                        ui::EventResult::ObjectRepair(uid) => {
                            profile.state.repair_building(&scene, &uid, wall(), || {
                                test_entropy.map(Ok).unwrap_or_else(save::entropy)
                            })
                        }
                        ui::EventResult::ObjectResearchCollect(command) => {
                            profile.state.begin_production_gather_ms(
                                &scene,
                                &command,
                                warzone_app_client::production_gather::Family::Research,
                                wall_ms(),
                            )
                        }
                        ui::EventResult::ObjectWorkshopCollect(command) => {
                            profile.state.begin_production_gather_ms(
                                &scene,
                                &command,
                                warzone_app_client::production_gather::Family::Material,
                                wall_ms(),
                            )
                        }
                        ui::EventResult::ObjectCollect(command) => {
                            profile.state.begin_production_gather_ms(
                                &scene,
                                &command,
                                warzone_app_client::production_gather::Family::Resource,
                                wall_ms(),
                            )
                        }
                        ui::EventResult::ObjectFinish(uid) => {
                            profile.state.finish_construction(&scene, &uid, wall(), || {
                                test_entropy.map(Ok).unwrap_or_else(save::entropy)
                            })
                        }
                        ui::EventResult::ResourceSpeedup(uid) => {
                            profile.state.speedup_production(&scene, &uid, wall())
                        }
                        ui::EventResult::RecipeUnlock(uid, id) => {
                            if scene
                                .workshop
                                .as_ref()
                                .is_some_and(|c| c.recipes.contains_key(&id))
                            {
                                profile
                                    .state
                                    .unlock_workshop_recipe(&scene, &uid, id, wall())
                            } else {
                                profile.state.unlock_recipe(&scene, &uid, id, wall())
                            }
                        }
                        ui::EventResult::UnlockShop(id) => {
                            profile.state.unlock_shop_item(&scene, id, wall())
                        }
                        ui::EventResult::BankClaim(id) => {
                            profile.state.claim_bank_offer(&scene, id, wall())
                        }
                        ui::EventResult::BuyMaterial(id) => {
                            profile.state.buy_material(&scene, id, wall())
                        }
                        ui::EventResult::BuyResearch(uid, id) => {
                            profile.state.buy_research(&scene, &uid, id, wall())
                        }
                        ui::EventResult::ResearchUnlock(uid, id) => {
                            profile.state.unlock_research(&scene, &uid, id, wall())
                        }
                        ui::EventResult::Claim(id) => profile.state.claim_quest(&scene, id, wall()),
                        ui::EventResult::ClaimAchievement(id, tier) => {
                            profile.state.claim_achievement(&scene, id, tier, wall())
                        }
                        ui::EventResult::Buy(id) => profile.state.buy_special(&scene, id, wall()),
                        ui::EventResult::Sell(uid) => profile.state.demolish(&scene, &uid, wall()),
                        ui::EventResult::ToggleAudio(key) => {
                            profile.state.toggle_audio(&scene, &key)
                        }
                        _ => unreachable!(),
                    };
                    match result {
                        Ok((next, text)) => {
                            profile.commit(&scene, next)?;
                            if upgrade_gather {
                                input.selected = None;
                                ui.selection = None;
                            }
                            message = text;
                            if let Some(id) = special_bought {
                                ui.special_bought(&scene, &profile.state, id);
                            }
                            if !keep_panel {
                                ui.kind = None;
                            }
                            if let Some(id) = shop_unlocked {
                                match editor.plan_building(&scene, &profile.state, &camera, id) {
                                    Ok(()) => {
                                        if id == 1027 {
                                            if let Some(e) = &editor.edit {
                                                let p = scene
                                                    .projection
                                                    .project(e.x as f64, e.y as f64);
                                                camera.x = p[0];
                                                camera.y =
                                                    p[1] - camera.height as f64 * 0.1 / camera.zoom;
                                            }
                                        }
                                        ui.kind = None;
                                    }
                                    Err(text) => ui.notice = Some(text),
                                }
                            }
                            if research_bought {
                                ui.research_bought();
                            }
                            if upgrade_bought {
                                ui.upgrade_bought();
                            }
                            let selected = input
                                .selected
                                .and_then(|i| world.entities.get(i))
                                .map(|e| e.uid.clone());
                            world.entities = profile.state.world_entities_at(&scene, Some(now))?;
                            input.selected = selected
                                .and_then(|uid| world.entities.iter().position(|e| e.uid == uid));
                        }
                        Err(text) => {
                            ui.notice = Some(text.clone());
                            message = text;
                        }
                    }
                    input.drag = None;
                    continue;
                }
            }
            match editor.event(
                &event,
                &camera,
                &scene,
                &world,
                &profile.state,
                input.selected,
                &mut message,
            ) {
                editor::ResultEvent::Pass => {}
                editor::ResultEvent::Consumed => {
                    input.drag = None;
                    continue;
                }
                editor::ResultEvent::Commit(edit) => {
                    let mut candidate = profile.state.clone();
                    candidate.camera = camera.saved_pose();
                    let result = if let Some((source, id)) = &editor.training {
                        candidate.place_trained_unit(
                            &scene,
                            source,
                            *id,
                            edit.x,
                            edit.y,
                            edit.direction,
                            wall(),
                            || test_entropy.map(Ok).unwrap_or_else(save::entropy),
                        )
                    } else if let Some(id) = editor.purchase {
                        if scene
                            .construction
                            .as_ref()
                            .is_some_and(|c| c.buildings.contains_key(&id))
                        {
                            let q = candidate.building_shortfall(&scene, id);
                            if !editor.pay_shortfall
                                && q.as_ref().is_ok_and(|q| !q.resources.is_empty())
                                && candidate
                                    .placement_type(&scene, id, "", edit.x, edit.y, edit.direction)
                                    .is_ok()
                            {
                                ui.building_shortfall(id);
                                input.drag = None;
                                continue;
                            }
                            editor.pay_shortfall = false;
                            candidate.buy_building_with_shortfall(
                                &scene,
                                id,
                                edit.x,
                                edit.y,
                                edit.direction,
                                wall(),
                                || test_entropy.map(Ok).unwrap_or_else(save::entropy),
                            )
                        } else {
                            let q = candidate.shop_entity_shortfall(&scene, id);
                            if !editor.pay_shortfall
                                && q.as_ref().is_ok_and(|q| !q.resources.is_empty())
                                && candidate
                                    .placement_type(&scene, id, "", edit.x, edit.y, edit.direction)
                                    .is_ok()
                            {
                                ui.building_shortfall(id);
                                input.drag = None;
                                continue;
                            }
                            editor.pay_shortfall = false;
                            candidate.buy_shop_entity_with_shortfall(
                                &scene,
                                id,
                                edit.x,
                                edit.y,
                                edit.direction,
                                wall(),
                                || test_entropy.map(Ok).unwrap_or_else(save::entropy),
                            )
                        }
                    } else {
                        candidate.apply_layout(&scene, &edit, wall())
                    };
                    match result {
                        Ok((next, text)) => {
                            profile.commit(&scene, next)?;
                            world.entities = profile.state.world_entities(&scene)?;
                            input.selected =
                                profile.state.selected.as_ref().and_then(|uid| {
                                    world.entities.iter().position(|e| &e.uid == uid)
                                });
                            message = text;
                            let repeat = editor
                                .purchase
                                .filter(|id| [3001, 3002, 3010, 3011].contains(id));
                            editor.cancel();
                            if let Some(id) = repeat {
                                // Source rebuyItem keeps placing fences until cancel.
                                if editor
                                    .plan_building(&scene, &profile.state, &camera, id)
                                    .is_ok()
                                {
                                    if let Some(next) = &mut editor.edit {
                                        next.x = edit.x;
                                        next.y = edit.y;
                                        next.direction = edit.direction;
                                    }
                                    input.selected = None;
                                }
                            }
                            layout_results.push(true);
                        }
                        Err(text) => {
                            message = text;
                            layout_results.push(false);
                        }
                    }
                    input.drag = None;
                    continue;
                }
            }
            match &event {
                Event::KeyDown {
                    keycode: Some(k @ (Keycode::U | Keycode::L | Keycode::N)),
                    repeat: false,
                    ..
                } => {
                    let id = if *k == Keycode::U {
                        1005
                    } else if *k == Keycode::N {
                        1020
                    } else {
                        1038
                    };
                    input.selected = world
                        .entities
                        .iter()
                        .position(|e| e.type_id == id && e.archived["state"] == 3);
                    if let Some(i) = input.selected {
                        let e = &world.entities[i];
                        let p = scene.projection.project(e.x, e.y);
                        camera.x = p[0];
                        camera.y = p[1];
                    } else {
                        message = "Сначала построй нужное здание через C".into();
                    }
                    recipe_index = 0;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Delete),
                    repeat: false,
                    ..
                } => {
                    if let Some(e) = input.selected.and_then(|i| world.entities.get(i)) {
                        if profile.state.can_clear_obstacle(&scene, &e.uid).is_ok() {
                            match profile.state.clear_obstacle(&scene, &e.uid, wall()) {
                                Ok((next, text)) => {
                                    profile.commit(&scene, next)?;
                                    message = text;
                                    input.selected = None;
                                    editor.cancel();
                                }
                                Err(text) => message = text,
                            }
                            input.drag = None;
                            continue;
                        }
                        match profile.state.sale_quote(&scene, &e.uid) {
                            Ok(_) => {
                                ui.begin_sale(e.uid.clone());
                                editor.cancel();
                            }
                            Err(text) => message = text,
                        }
                    }
                    input.drag = None;
                    continue;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::H),
                    repeat: false,
                    ..
                } => {
                    input.selected = world.entities.iter().position(|e| e.type_id == 1027);
                    if let Some(i) = input.selected {
                        let e = &world.entities[i];
                        let p = scene.projection.project(e.x, e.y);
                        camera.x = p[0];
                        camera.y = p[1];
                    }
                    recipe_index = 0;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Tab),
                    repeat: false,
                    ..
                } => {
                    recipe_index = recipe_index.wrapping_add(1);
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    ..
                } => {
                    activate = true;
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if scene.interface.is_none() && *y >= camera.height as i32 - 138 => {
                    input.panel_press = panel_button(*x, *y, &camera);
                    input.drag = None;
                    continue;
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if scene.interface.is_none() && *y >= camera.height as i32 - 138 => {
                    let pressed = input.panel_press.take();
                    input.drag = None;
                    if pressed.is_some() && pressed == panel_button(*x, *y, &camera) {
                        if pressed == Some(0) {
                            activate = true;
                        }
                        if pressed == Some(1) {
                            recipe_index = recipe_index.wrapping_add(1);
                        }
                    }
                    if !activate {
                        continue;
                    }
                }
                _ => {}
            }
            if activate {
                if let Some(i) = input.selected {
                    let uid = &world.entities[i].uid;
                    if let Some(prompt) = profile.state.collection_capacity_prompt(&scene, uid, now)
                    {
                        ui.offer_capacity(prompt);
                        input.selected = None;
                        input.drag = None;
                        continue;
                    }
                    let mut candidate = profile.state.clone();
                    candidate.camera = camera.saved_pose();
                    candidate.selected = Some(uid.clone());
                    let command = scene
                        .economy
                        .as_ref()
                        .and_then(|c| candidate.command(c, uid, recipe_index));
                    // ProductionDialog asks for the missing inputs before it
                    // starts. Do not let a negative balance reach adjust().
                    if scene.interface.is_some() {
                        let start = candidate
                            .research_command(&scene, uid, recipe_index)
                            .or_else(|| candidate.workshop_command(&scene, uid, recipe_index))
                            .or_else(|| command.clone());
                        if let Some(cmd @ warzone_app_client::production::Command::Start { .. }) =
                            start
                        {
                            if candidate
                                .production_shortfall(&scene, &cmd)
                                .is_ok_and(|q| !q.resources.is_empty())
                            {
                                ui.kind = Some(
                                    if candidate
                                        .research_command(&scene, uid, recipe_index)
                                        .is_some()
                                    {
                                        ui::Kind::Research
                                    } else if candidate
                                        .workshop_command(&scene, uid, recipe_index)
                                        .is_some()
                                    {
                                        ui::Kind::Production
                                    } else {
                                        ui::Kind::ResourceProduction
                                    },
                                );
                                ui.production_shortfall = Some(cmd);
                                input.drag = None;
                                continue;
                            }
                        }
                    }
                    let result = if candidate.repair_pending(&scene, uid) {
                        Some(candidate.repair_building(&scene, uid, wall(), || {
                            test_entropy.map(Ok).unwrap_or_else(save::entropy)
                        }))
                    } else if candidate.construction_pending(&scene, uid) {
                        Some(candidate.finish_construction(&scene, uid, wall(), || {
                            test_entropy.map(Ok).unwrap_or_else(save::entropy)
                        }))
                    } else if let Some(research) =
                        candidate.research_command(&scene, uid, recipe_index)
                    {
                        Some(
                            if scene.interface.is_some()
                                && matches!(
                                    research,
                                    warzone_app_client::production::Command::Collect { .. }
                                )
                            {
                                candidate.begin_production_gather_ms(
                                    &scene,
                                    &research,
                                    warzone_app_client::production_gather::Family::Research,
                                    wall_ms(),
                                )
                            } else {
                                candidate.research_apply(&scene, &research, wall())
                            },
                        )
                    } else if let Some(work) = candidate.workshop_command(&scene, uid, recipe_index)
                    {
                        if let warzone_app_client::production::Command::Collect {
                            recipe,
                            started,
                            ..
                        } = &work
                        {
                            if scene.workshop.as_ref().unwrap().recipes[recipe].unit() {
                                let source = warzone_app_client::production::ProductionSource {
                                    uid: uid.clone(),
                                    started: *started,
                                };
                                if ui.begin_training_instruction(&candidate, &source, *recipe) {
                                    candidate
                                        .systems
                                        .as_mut()
                                        .unwrap()
                                        .set("FirstUnitPlacingDialog".into(), 0);
                                    profile.commit(&scene, candidate)?;
                                    input.drag = None;
                                    continue;
                                }
                                match editor.plan_training(&scene,&candidate,&camera,source,*recipe,wall()) {
                                    Ok(())=>message="Выбери место для отряда · Enter: разместить · Esc: отменить".into(),
                                    Err(text)=>{message=text;action_results.push(false);}
                                }
                                continue;
                            }
                        }
                        Some(
                            if scene.interface.is_some()
                                && matches!(
                                    work,
                                    warzone_app_client::production::Command::Collect { .. }
                                )
                            {
                                candidate.begin_production_gather_ms(
                                    &scene,
                                    &work,
                                    warzone_app_client::production_gather::Family::Material,
                                    wall_ms(),
                                )
                            } else {
                                candidate.workshop_apply(&scene, &work, wall(), || {
                                    test_entropy.map(Ok).unwrap_or_else(save::entropy)
                                })
                            },
                        )
                    } else if let Some(command) = command {
                        Some(
                            if let warzone_app_client::production::Command::Start { uid, recipe } =
                                &command
                            {
                                if candidate.needs_recipe_unlock(&scene, *recipe) {
                                    candidate.unlock_recipe(&scene, uid, *recipe, wall())
                                } else {
                                    candidate.apply(&scene, &command, wall(), || {
                                        test_entropy.map(Ok).unwrap_or_else(save::entropy)
                                    })
                                }
                            } else if scene.interface.is_some() {
                                candidate.begin_production_gather_ms(
                                    &scene,
                                    &command,
                                    warzone_app_client::production_gather::Family::Resource,
                                    wall_ms(),
                                )
                            } else {
                                candidate.apply(&scene, &command, wall(), || {
                                    test_entropy.map(Ok).unwrap_or_else(save::entropy)
                                })
                            },
                        )
                    } else {
                        None
                    };
                    if let Some(result) = result {
                        match result {
                            Ok((next, text)) => {
                                let gathering = next.gathering(uid);
                                profile.commit(&scene, next)?;
                                if gathering
                                    || profile.state.actions.last().is_some_and(|a| {
                                        matches!(a.kind.as_str(), "start" | "workshop_start")
                                    })
                                {
                                    input.selected = None;
                                    ui.selection = None;
                                }
                                message = text;
                                action_results.push(true);
                            }
                            Err(text) => {
                                ui.notice = Some(text.clone());
                                message = text;
                                action_results.push(false);
                            }
                        }
                    }
                }
                continue;
            }
            if !map_input_applied && !input.event(event, &mut camera, &world) {
                break 'app;
            }
        }
        // Read actual output dimensions as well as resize events: high DPI and
        // synthetic event tests must never leave the camera with a stale viewport.
        let (w, h) = canvas.output_size()?;
        camera.width = w;
        camera.height = h;
        let (detail, action) = input
            .selected
            .filter(|_| scene.interface.is_none() && tactical.is_none())
            .map(|i| {
                profile
                    .state
                    .research_panel(&scene, &world.entities[i].uid, recipe_index, wall())
                    .or_else(|| {
                        profile.state.workshop_panel(
                            &scene,
                            &world.entities[i].uid,
                            recipe_index,
                            wall(),
                        )
                    })
                    .unwrap_or_else(|| {
                        profile
                            .state
                            .panel(&scene, &world.entities[i].uid, recipe_index, wall())
                    })
            })
            .unwrap_or_default();
        // Refresh after transactions too: new objects are visible/pickable in
        // the same frame as their atomic purchase/finish, with selection by UID.
        if tactical.is_none() && profile.state.revision != view_revision {
            let selected = input
                .selected
                .and_then(|i| world.entities.get(i))
                .map(|e| e.uid.clone());
            world.entities = profile.state.world_entities_at(&scene, Some(now))?;
            input.selected =
                selected.and_then(|uid| world.entities.iter().position(|e| e.uid == uid));
            view_revision = profile.state.revision;
        }
        let (detail, action) = if input
            .selected
            .map(|i| &world.entities[i].uid)
            .is_some_and(|uid| profile.state.repair_pending(&scene, uid))
        {
            (
                "Здание повреждено · производство недоступно".into(),
                "Ремонт".into(),
            )
        } else if let Some(uid) = input
            .selected
            .map(|i| &world.entities[i].uid)
            .filter(|uid| profile.state.construction_pending(&scene, uid))
        {
            let v = &profile.state.entities[uid];
            let left = (v["buildStartTime"].as_i64().unwrap()
                + v["buildTotalTime"].as_i64().unwrap()
                - now)
                .max(0);
            (
                if left == 0 {
                    "Постройка готова: заверши строительство".into()
                } else {
                    format!("Строительство · осталось {left} сек.")
                },
                if left == 0 {
                    "Завершить".into()
                } else {
                    String::new()
                },
            )
        } else {
            (detail, action)
        };
        let preview = editor.preview(&scene, &world, &profile.state);
        if world_check.is_some() && [15, 25, 40].contains(&count) {
            if let Some((e, valid, _)) = &preview {
                layout_previews.push(serde_json::json!({"frame":count,"uid":e.uid,"x":e.x,"y":e.y,"direction":e.direction,"valid":valid}));
            }
        }
        let (detail, action) = if let Some((_, _, hint)) = &preview {
            (hint.clone(), "Сохранить".into())
        } else {
            (detail, action)
        };
        let prepare_started = Instant::now();
        let prepare_ms;
        let draw_ms;
        if let Some(controller) = &mut tactical {
            controller.sync(&scene, &profile.state)?;
            if let Some(a) = &mut audio {
                a.sync_presentation(&profile.state, controller.result_presentation_ready());
                for cue in controller.take_sounds() {
                    a.unit_event(cue.type_id, cue.event);
                }
            }
            for cue in controller.take_reward_sounds() {
                if let Some(a) = &mut audio {
                    a.event(cue);
                }
            }
            prepare_ms = prepare_started.elapsed().as_secs_f64() * 1000.;
            let draw_started = Instant::now();
            draws = controller.draw(
                &mut art,
                &mut canvas,
                &scene,
                &profile.state,
                wall(),
                started.elapsed().as_secs_f64(),
                &message,
            )?;
            draw_ms = draw_started.elapsed().as_secs_f64() * 1000.;
        } else {
            if let Some(a) = &mut audio {
                a.sync(&profile.state);
            }
            ui.selection = input
                .selected
                .and_then(|i| world.entities.get(i))
                .map(|e| e.uid.clone());
            ui.editing = editor.edit.is_some();
            ui.placement_only = editor.training.is_some();
            ui.update_time_ms(
                &scene,
                &profile.state,
                profile.state.callback_game_ms(&scene, wall_ms())?,
            );
            if let Some(uid) = ui.update_guide_focus(&scene, &profile.state, &camera) {
                input.selected = world.entities.iter().position(|e| e.uid == uid);
            }
            ui.prepare_reward_camera(&scene, &camera);
            let markers = ui.world_markers(&scene, &profile.state, &camera);
            let original =
                ui.original_view_with_markers(&scene, &profile.state, &camera, Some(&markers));
            // Cosmetic flight admission is independent of save commits and
            // cannot award anything. Drain even if no audio device is present.
            for cue in ui.take_reward_sounds() {
                if let Some(a) = &mut audio {
                    a.event(cue);
                }
            }
            prepare_ms = prepare_started.elapsed().as_secs_f64() * 1000.;
            let draw_started = Instant::now();
            draws = art.draw(
                &mut canvas,
                &world,
                &camera,
                started.elapsed().as_secs_f64(),
                input.selected,
                render::Hud {
                    original: original.as_ref(),
                    world_markers: Some(&markers),
                    motion: None,
                    tactical: false,
                    navigation: None,
                    zones: vec![],
                    values: ui.reward_hud_values(&scene, &profile.state, wall()),
                    preview: preview.as_ref().map(|(e, valid, _)| (e, *valid)),
                    editing: editor.edit.is_some(),
                    energy: format!(
                        "{}/{}",
                        profile.state.energy(
                            &scene,
                            profile
                                .state
                                .clock(scene.economy.as_ref().unwrap(), wall())?
                        ),
                        profile.state.max_energy(&scene)
                    ),
                    quests: ui.view(
                        &scene,
                        &profile.state,
                        profile
                            .state
                            .clock(scene.economy.as_ref().unwrap(), wall())?,
                    ),
                    resources: &profile.state.resources,
                    detail: &detail,
                    action: &action,
                    message: if preview.is_some() {
                        "ЛКМ или Enter: сохранить · R: поворот · Esc: отмена"
                    } else {
                        &message
                    },
                },
            )?;
            draw_ms = draw_started.elapsed().as_secs_f64() * 1000.;
        }
        let screenshot_started = Instant::now();
        if let Some(path) = &screen {
            if count == 0
                || (check && [16, 36, 70].contains(&count))
                || (world_check.is_some() && [15, 25, 40].contains(&count))
                || (construction_check.is_some() && [12, 28, 52, 68].contains(&count))
                || (lifecycle_check.is_some() && [8, 16, 26, 52].contains(&count))
                || (workshop_check.is_some() && [20, 32, 40, 52].contains(&count))
                || mission_script.as_ref().is_some_and(|s| {
                    s["screenshots"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v.as_u64() == Some(count)))
                })
                || (frames > 0 && count + 1 == frames)
            {
                let path = if frames > 0 && count + 1 == frames {
                    path.clone()
                } else {
                    path.with_file_name(format!(
                        "{}-{count:03}.png",
                        path.file_stem().unwrap().to_string_lossy()
                    ))
                };
                render::screenshot(&canvas, &path)?;
                if mission_script.is_some() {
                    motion_samples.push(serde_json::json!({"frame":count,"image":path,
                        "motion":tactical.as_ref().and_then(|u|u.motion_report())}));
                }
                screenshots.push(path);
            }
        }
        let screenshot_ms = screenshot_started.elapsed().as_secs_f64() * 1000.;
        let present_started = Instant::now();
        canvas.present();
        let present_ms = present_started.elapsed().as_secs_f64() * 1000.;
        let phase = if tactical.is_some() {
            match profile
                .state
                .mission
                .as_ref()
                .map(|m| m.battle.phase.as_str())
            {
                Some("placement") => "deployment",
                Some("win" | "defeat" | "finished") => "battle_result",
                _ => "battle",
            }
        } else {
            match ui.kind {
                None => {
                    if ui.quest_focus_active() {
                        "base_tutorial"
                    } else {
                        "base"
                    }
                }
                Some(ui::Kind::Construction | ui::Kind::Shop) => "shop",
                Some(
                    ui::Kind::Production | ui::Kind::ResourceProduction | ui::Kind::ResourceSpeedup,
                ) => "production",
                Some(ui::Kind::Research | ui::Kind::Upgrades) => "research_upgrades",
                Some(ui::Kind::Missions) => "campaign_map",
                Some(ui::Kind::Quests) => "quests",
                Some(ui::Kind::Achievements | ui::Kind::Leagues | ui::Kind::LeaguesHelp) => {
                    "achievements_leagues"
                }
                _ => "other_dialogs",
            }
        };
        frame_metrics.observe_parts(
            phase,
            tick.elapsed().as_secs_f64() * 1000.,
            &[
                (
                    "update",
                    prepare_started.duration_since(tick).as_secs_f64() * 1000.,
                ),
                ("presentation", prepare_ms),
                ("draw", draw_ms),
                ("capture", screenshot_ms),
                ("present", present_ms),
            ],
        );
        count += 1;
        if period.elapsed() >= Duration::from_secs(1) {
            let fps = (count - last_frames) as f64 / period.elapsed().as_secs_f64();
            fps_samples.push(fps);
            canvas
                .window_mut()
                .set_title(&format!("Warzone | {fps:.1} FPS"))
                .map_err(|e| e.to_string())?;
            period = Instant::now();
            last_frames = count;
        }
        let script_finished = mission_script.as_ref().is_some_and(|script| {
            let Some(tail) = script["end_after_steps"]
                .as_u64()
                .filter(|n| (1..=600).contains(n))
            else {
                return false;
            };
            let Some(last) = script["steps"]
                .as_array()
                .and_then(|steps| steps.iter().filter_map(|s| s["frame"].as_u64()).max())
            else {
                return false;
            };
            count.saturating_sub(script_camera_wait_frames) >= last.saturating_add(tail)
                && tactical
                    .as_ref()
                    .is_none_or(|u| !u.script_visuals_pending())
        });
        // Browsers cannot await IDBFS during page unload. Persist a settled
        // camera/selection as well as gameplay actions; avoid a write per frame.
        #[cfg(target_os = "emscripten")]
        {
            let pose = (camera.saved_pose(), input.selected);
            if pose != observed_pose {
                observed_pose = pose;
                pose_changed = Instant::now();
            }
            if pose != saved_pose && pose_changed.elapsed() >= Duration::from_millis(500) {
                let mut next = profile.state.clone();
                next.camera = pose.0;
                next.selected = pose
                    .1
                    .and_then(|i| world.entities.get(i))
                    .map(|e| e.uid.clone());
                profile.commit(&scene, next)?;
                saved_pose = pose;
            }
        }
        if frames > 0 && (count >= frames || script_finished) {
            if script_finished && count < frames {
                if let Some(path) = &screen {
                    render::screenshot(&canvas, path)?;
                    screenshots.push(path.clone());
                }
            }
            break;
        }
        #[cfg(target_os = "emscripten")]
        warzone_app_client::browser::yield_frame(
            Duration::from_secs_f64(1. / 60.)
                .checked_sub(tick.elapsed())
                .unwrap_or(Duration::from_millis(1)),
        );
        #[cfg(not(target_os = "emscripten"))]
        if !uncapped_diagnostic && !vsync_active {
            if let Some(rest) = Duration::from_secs_f64(1. / 60.).checked_sub(tick.elapsed()) {
                warzone_app_client::browser::yield_frame(rest);
            }
        }
    }
    if check
        && (input.pans == 0
            || input.zooms == 0
            || input.picks == 0
            || input.selected.is_none()
            || input.resizes == 0)
    {
        return Err("Native event self-check did not complete".into());
    }
    let mut final_state = profile.state.clone();
    if let Some(controller) = &tactical {
        controller.save_camera(&mut final_state);
    }
    final_state.camera = camera.saved_pose();
    final_state.selected = input.selected.map(|i| world.entities[i].uid.clone());
    if serde_json::to_value(&final_state).map_err(|e| e.to_string())?
        != serde_json::to_value(&profile.state).map_err(|e| e.to_string())?
    {
        profile.commit(&scene, final_state)?;
    }
    if let Some(kind) = production_check.as_deref() {
        let expected = match kind {
            "start" | "collect" => vec![true],
            "wait" => vec![false],
            _ => Vec::new(),
        };
        if action_results != expected {
            return Err(format!("Production check {kind} failed: {message}"));
        }
    }
    if let Some(mode) = world_check.as_deref() {
        let expected = match mode {
            "move" => vec![false, true],
            "rotate" => vec![true],
            _ => vec![],
        };
        if layout_results != expected {
            return Err(format!("World check failed: {message}; {layout_results:?}"));
        }
    }
    if let Some(mode) = construction_check.as_deref() {
        let expected = match mode {
            "place" => vec![false, true],
            _ => vec![],
        };
        let actions = match mode {
            "wait" => vec![false],
            "finish" | "produce" => vec![true],
            _ => vec![],
        };
        if layout_results != expected || action_results != actions {
            return Err(format!(
                "Construction SDL check failed: {message}; layout={layout_results:?},actions={action_results:?}"
            ));
        }
    }
    if systems_check.as_deref() == Some("claim")
        && (profile.state.resources["level"] != 6
            || profile.state.systems.as_ref().unwrap().status(4) != 99)
    {
        return Err(format!("Systems GUI check failed: {message}"));
    }
    let mut result = serde_json::json!({"format":"warzone-desktop-run-v1","frames":count,"mission":profile.state.mission,
        "wall_seconds":started.elapsed().as_secs_f64(),"fps_samples":fps_samples,"renderer":canvas.info().name,"motion_samples":motion_samples,"audio":audio.as_mut().map(|a|a.report()),"audio_error":audio_error,"tactical_interface":tactical.as_ref().and_then(|u|u.source_report(&scene,&profile.state)),"source_interface":ui.source_report(&scene,&profile.state,&camera),"source_button_layout":art.source_button_report(ui.original_view(&scene,&profile.state,&camera).as_ref())?,
        "entities":world.entities.len(),"textures":scene.textures.len(),"texture_cache":art.texture_report(),"visible_draws_last_frame":draws,
        "selected":input.selected.map(|i|&world.entities[i].uid),"camera":[camera.x,camera.y,camera.zoom],
        "input":{"pans":input.pans,"zooms":input.zooms,"picks":input.picks,"resizes":input.resizes},
        "self_check":check,"screenshots":screenshots,"guest_cpu":false,"mach_o_loader":false,
        "production_check":production_check,"systems_check":systems_check,"action_results":action_results,"message":message,
        "world_check":world_check,"layout_results":layout_results,"layout_previews":layout_previews,
        "layout_edit_active":editor.edit.is_some(),"training_placement_active":editor.training.is_some(),
        "construction_check":construction_check,
        "lifecycle_check":lifecycle_check,
        "workshop_check":workshop_check,
        "energy":profile.state.energy(&scene,profile.state.clock(scene.economy.as_ref().unwrap(),wall())?),
        "active_quests":profile.state.active_quests(&scene).iter().map(|q|q.id).collect::<Vec<_>>(),
        "resources":profile.state.resources,"inventory":profile.state.inventory,"revision":profile.state.revision,
        "note":"Native resources, levels, energy and quests; full game and visual parity pending"});
    result["script_camera_wait_frames"] = script_camera_wait_frames.into();
    result["frame_timings"] = frame_metrics.report();
    result["uncapped_diagnostic"] = uncapped_diagnostic.into();
    result["vsync_active"] = vsync_active.into();
    result["manual_frame_pacing"] = (!uncapped_diagnostic && !vsync_active).into();
    result["quest_camera_samples"] =
        serde_json::to_value(&quest_camera_samples).map_err(|e| e.to_string())?;
    result["loading_frames"] = loading_frames.into();
    result["reward_hud_texts"] = serde_json::to_value(
        tactical
            .as_ref()
            .map_or_else(
                || ui.reward_hud_values(&scene, &profile.state, wall()),
                |t| t.reward_hud_values(&scene, &profile.state, wall()),
            )
            .texts,
    )
    .map_err(|e| e.to_string())?;
    result["mission_action_trace"] =
        serde_json::to_value(&mission_action_trace).map_err(|e| e.to_string())?;
    if let Some(path) = report {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap())
            .map_err(|e| e.to_string())?;
    }
    println!("{result}");
    Ok(())
}
fn script(
    frame: u64,
    events: &sdl2::EventSubsystem,
    id: u32,
    camera: &Camera,
    scene: &Scene,
) -> Result<(), String> {
    let button = |down, x, y| {
        if down {
            Event::MouseButtonDown {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        } else {
            Event::MouseButtonUp {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        }
    };
    match frame {
        10 => events.push_event(Event::MouseWheel {
            timestamp: 0,
            window_id: id,
            which: 0,
            x: 0,
            y: 1,
            direction: MouseWheelDirection::Normal,
            precise_x: 0.,
            precise_y: 1.,
            mouse_x: 300,
            mouse_y: 250,
        })?,
        20 => events.push_event(button(true, 300, 300))?,
        21 => events.push_event(Event::MouseMotion {
            timestamp: 0,
            window_id: id,
            which: 0,
            mousestate: MouseState::from_sdl_state(1),
            x: 380,
            y: 340,
            xrel: 80,
            yrel: 40,
        })?,
        22 => events.push_event(button(false, 380, 340))?,
        50 => events.push_event(Event::KeyDown {
            timestamp: 0,
            window_id: id,
            keycode: Some(Keycode::Home),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat: false,
        })?,
        60 | 61 => {
            let e = scene
                .entities
                .iter()
                .find(|e| e.type_id == 1000)
                .ok_or("Self-check requires headquarters")?;
            let p = camera.screen(scene.projection.project(e.x, e.y));
            events.push_event(button(
                frame == 60,
                p[0].round() as i32,
                p[1].round() as i32,
            ))?;
        }
        _ => {}
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("warzone-app-client: {e}");
        std::process::exit(1);
    }
}

// Private deterministic SDL scenario; only --world-check diagnostics call this.
fn layout_script(
    frame: u64,
    mode: &str,
    events: &sdl2::EventSubsystem,
    id: u32,
    camera: &mut Camera,
    view: &Scene,
    grab: Option<([f64; 2], [f64; 2])>,
) -> Result<(), String> {
    let uid = "28A3D7B66192873A6BFE";
    let e = view
        .entities
        .iter()
        .find(|e| e.uid == uid)
        .ok_or("World check needs saved tent")?;
    if frame == 0 {
        let p = view.projection.project(43.0, 48.0);
        camera.x = p[0];
        camera.y = p[1];
        camera.zoom = 0.85;
    }
    let point = |x: f64, y: f64| {
        let p = camera.screen(view.projection.project(x, y));
        (p[0].round() as i32, p[1].round() as i32)
    };
    let key = |keycode| Event::KeyDown {
        timestamp: 0,
        window_id: id,
        keycode: Some(keycode),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
    };
    let mouse = |down: bool, x: i32, y: i32| {
        if down {
            Event::MouseButtonDown {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        } else {
            Event::MouseButtonUp {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        }
    };
    if frame == 5 || frame == 6 {
        let (x, y) = point(e.x, e.y);
        events.push_event(mouse(frame == 5, x, y))?;
    }
    if mode == "view" {
        return Ok(());
    }
    if frame == 10 {
        events.push_event(key(if mode == "rotate" {
            Keycode::R
        } else {
            Keycode::M
        }))?;
    }
    if mode != "rotate" && (frame == 20 || frame == 35) {
        let (x, y) = if frame == 20 && mode == "move" {
            point(47.0, 44.0)
        } else {
            point(43.0, 52.0)
        };
        let (from, anchor) = grab.ok_or("World diagnostic has no editable preview")?;
        let to = [
            x as f64 + from[0] - anchor[0],
            y as f64 + from[1] - anchor[1],
        ];
        events.push_event(mouse(true, from[0].round() as i32, from[1].round() as i32))?;
        events.push_event(Event::MouseMotion {
            timestamp: 0,
            window_id: id,
            which: 0,
            mousestate: MouseState::from_sdl_state(1),
            x: to[0].round() as i32,
            y: to[1].round() as i32,
            xrel: (to[0] - from[0]).round() as i32,
            yrel: (to[1] - from[1]).round() as i32,
        })?;
        events.push_event(mouse(false, to[0].round() as i32, to[1].round() as i32))?;
    }
    if mode == "move" && frame == 28 {
        events.push_event(key(Keycode::Return))?;
    }
    if frame == 45 {
        match mode {
            "cancel" => events.push_event(key(Keycode::Escape))?,
            "rotate" => events.push_event(key(Keycode::Return))?,
            _ => events.push_event(mouse(true, 70, camera.height as i32 - 25))?,
        }
    }
    if mode == "move" && frame == 46 {
        events.push_event(mouse(false, 70, camera.height as i32 - 25))?;
    }
    Ok(())
}

fn construction_script(
    frame: u64,
    mode: &str,
    events: &sdl2::EventSubsystem,
    id: u32,
    camera: &mut Camera,
    view: &Scene,
    seed: &Scene,
    state: &warzone_app_client::production::State,
) -> Result<(), String> {
    if frame == 0 {
        let p = view.projection.project(43.0, 48.0);
        camera.x = p[0];
        camera.y = p[1];
        camera.zoom = 0.85;
    }
    let key = |k| Event::KeyDown {
        timestamp: 0,
        window_id: id,
        keycode: Some(k),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
    };
    let mouse = |down: bool, x: i32, y: i32| {
        if down {
            Event::MouseButtonDown {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        } else {
            Event::MouseButtonUp {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        }
    };
    let point = |x: f64, y: f64| {
        let p = camera.screen(view.projection.project(x, y));
        (p[0].round() as i32, p[1].round() as i32)
    };
    if matches!(mode, "place" | "cancel") {
        if frame == 5 {
            events.push_event(key(Keycode::C))?;
        }
        if frame == 8 {
            let n = state
                .building_items(seed)
                .iter()
                .position(|b| b.id == 1007)
                .ok_or("Missing oil shop item")?;
            for _ in 0..n {
                events.push_event(key(Keycode::Right))?;
            }
        }
        if frame == 15 {
            events.push_event(key(Keycode::Return))?;
        }
        if frame == 22 || frame == 40 {
            let (x, y) = if frame == 22 {
                point(47.0, 44.0)
            } else {
                point(43.0, 52.0)
            };
            events.push_event(Event::MouseMotion {
                timestamp: 0,
                window_id: id,
                which: 0,
                mousestate: MouseState::from_sdl_state(0),
                x,
                y,
                xrel: 0,
                yrel: 0,
            })?;
        }
        if mode == "place" && frame == 30 {
            events.push_event(key(Keycode::Return))?;
        }
        if frame == 60 {
            if mode == "cancel" {
                events.push_event(key(Keycode::Escape))?;
            } else {
                events.push_event(mouse(true, 70, camera.height as i32 - 25))?;
            }
        }
        if frame == 61 && mode == "place" {
            events.push_event(mouse(false, 70, camera.height as i32 - 25))?;
        }
    } else {
        if frame == 5 || frame == 6 {
            let uid = state
                .actions
                .iter()
                .find(|a| a.kind == "building_buy")
                .ok_or("Needs purchased building")?
                .uid
                .as_str();
            let e = view
                .entities
                .iter()
                .find(|e| e.uid == uid)
                .ok_or("Purchased building not drawn")?;
            let (x, y) = point(e.x, e.y);
            events.push_event(mouse(frame == 5, x, y))?;
        }
        if frame == 30 && matches!(mode, "wait" | "finish" | "produce") {
            events.push_event(key(Keycode::Return))?;
        }
    }
    Ok(())
}

fn lifecycle_script(
    frame: u64,
    mode: &str,
    events: &sdl2::EventSubsystem,
    id: u32,
    camera: &mut Camera,
    view: &Scene,
    source_ui: &ui::Ui,
    state: &warzone_app_client::production::State,
) -> Result<(), String> {
    let key = |keycode| Event::KeyDown {
        timestamp: 0,
        window_id: id,
        keycode: Some(keycode),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
    };
    let mouse = |down: bool, x, y| {
        if down {
            Event::MouseButtonDown {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        } else {
            Event::MouseButtonUp {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        }
    };
    if mode != "view" {
        let e = view.entities.iter().find(|e| e.uid == "389788198448260844");
        if frame == 0 {
            let e = e.ok_or("Missing lifecycle fixture")?;
            let p = view.projection.project(e.x, e.y);
            camera.x = p[0];
            camera.y = p[1];
        }
        if frame == 4 || frame == 5 {
            events.push_event(mouse(
                frame == 4,
                (camera.width / 2) as i32,
                (camera.height / 2) as i32,
            ))?;
        }
        if frame == 10 {
            events.push_event(key(if mode == "repair" {
                Keycode::Return
            } else {
                Keycode::Delete
            }))?;
        }
        if mode == "cancel" && frame == 22 {
            events.push_event(key(Keycode::Escape))?;
        }
        if mode == "sell" && matches!(frame, 12 | 13 | 22 | 23) {
            let point = if view
                .interface
                .as_ref()
                .is_some_and(|c| c.symbols.contains_key("message"))
            {
                let v = source_ui
                    .original_view(view, state, camera)
                    .ok_or("Missing sale view")?;
                let h = v
                    .hits
                    .iter()
                    .find(|h| h.action == warzone_app_client::interface::Action::Confirm)
                    .ok_or("Missing source sale confirmation")?;
                [
                    (h.rect[0] + h.rect[2] / 2.).round() as i32,
                    (h.rect[1] + h.rect[3] / 2.).round() as i32,
                ]
            } else {
                let r = render::modal_layout(camera).claim;
                [r.x() + 30, r.y() + 12]
            };
            // A press elsewhere followed by release on Sell must never confirm.
            if frame == 12 {
                events.push_event(mouse(true, 1, 1))?;
            }
            if frame == 13 {
                events.push_event(mouse(false, point[0], point[1]))?;
            }
            if frame == 22 || frame == 23 {
                events.push_event(mouse(frame == 22, point[0], point[1]))?;
            }
        }
    }
    Ok(())
}

fn workshop_script(
    frame: u64,
    mode: &str,
    events: &sdl2::EventSubsystem,
    id: u32,
    camera: &mut Camera,
    view: &Scene,
    seed: &Scene,
    state: &warzone_app_client::production::State,
) -> Result<(), String> {
    let key = |keycode| Event::KeyDown {
        timestamp: 0,
        window_id: id,
        keycode: Some(keycode),
        scancode: None,
        keymod: Mod::NOMOD,
        repeat: false,
    };
    let mouse = |down: bool, x, y| {
        if down {
            Event::MouseButtonDown {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        } else {
            Event::MouseButtonUp {
                timestamp: 0,
                window_id: id,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            }
        }
    };
    if mode == "view" {
        return Ok(());
    }
    if frame == 0 {
        let p = if mode == "lab-buy" {
            view.projection.project(49.0, 50.0)
        } else {
            let e = if mode.starts_with("train-") {
                view.entities.iter().find(|e| e.uid == "335977976794178293")
            } else if mode.starts_with("research-") {
                view.entities.iter().find(|e| e.type_id == 1020)
            } else {
                view.entities.iter().find(|e| e.type_id == 1038)
            }
            .ok_or("Missing workshop check building")?;
            view.projection.project(e.x, e.y)
        };
        camera.x = p[0];
        camera.y = p[1];
    }
    if mode == "lab-buy" {
        if frame == 5 {
            events.push_event(key(Keycode::C))?;
        }
        if frame == 8 {
            let n = state
                .building_items(seed)
                .iter()
                .position(|b| b.id == 1038)
                .ok_or("Missing lab offer")?;
            for _ in 0..n {
                events.push_event(key(Keycode::Right))?;
            }
        }
    } else if frame == 4 || frame == 5 {
        events.push_event(mouse(
            frame == 4,
            (camera.width / 2) as i32,
            (camera.height / 2) as i32,
        ))?;
    }
    if (mode.starts_with("steel-") || mode == "train-commando") && frame == 12 {
        events.push_event(key(Keycode::Tab))?;
    }
    if frame == 15 {
        events.push_event(key(Keycode::Return))?;
    }
    let placement = matches!(mode, "lab-buy" | "train-cancel" | "train-place");
    if mode == "train-place" && (frame == 20 || frame == 21) {
        events.push_event(key(Keycode::R))?;
    }
    if placement && (frame == 25 || frame == 36) {
        let (x, y) = if mode == "lab-buy" {
            (49.0, 53.0)
        } else if frame == 25 {
            (47.0, 44.0)
        } else {
            (43.0, 52.0)
        };
        let p = camera.screen(view.projection.project(x, y));
        events.push_event(Event::MouseMotion {
            timestamp: 0,
            window_id: id,
            which: 0,
            mousestate: MouseState::from_sdl_state(0),
            x: p[0].round() as i32,
            y: p[1].round() as i32,
            xrel: 0,
            yrel: 0,
        })?;
    }
    if mode == "train-place" && frame == 30 {
        events.push_event(key(Keycode::Return))?;
    }
    if placement && frame == 45 {
        if mode == "train-cancel" {
            events.push_event(key(Keycode::Escape))?;
        } else {
            events.push_event(mouse(true, 70, camera.height as i32 - 25))?;
        }
    }
    if matches!(mode, "lab-buy" | "train-place") && frame == 46 {
        events.push_event(mouse(false, 70, camera.height as i32 - 25))?;
    }
    Ok(())
}
