//! Desktop controls for the native mission model. No base actions leak through.
use crate::render::{Art, Hud, PanelView, modal_layout};
use sdl2::{
    event::Event,
    keyboard::Keycode,
    mouse::MouseButton,
    rect::{Point, Rect},
    render::Canvas,
    video::Window,
};
use warzone_app_client::interface::{Action as SourceAction, View as SourceView};
use warzone_app_client::{Camera, Scene, mission::Command, production::State};
struct CardGesture {
    index: usize,
    start: [i32; 2],
    point: [i32; 2],
    scroll: f64,
    mode: u8, // pending, horizontal list pan, or vertical deployment
}
impl CardGesture {
    fn update(&mut self, point: [i32; 2], list_top: f64) {
        self.point = point;
        let (dx, dy) = (point[0] - self.start[0], point[1] - self.start[1]);
        if dx.abs() + dy.abs() <= crate::input_events::slop(8) {
            return;
        }
        // A sideways start may scroll the cards, but leaving the troop strip
        // transfers that same captured gesture to deployment. The first few
        // pixels of hand movement must not permanently lock it to scrolling.
        if (point[1] as f64) < list_top {
            self.mode = 2;
        } else if self.mode == 0 {
            self.mode = if dx.abs() > 2 * dy.abs() { 1 } else { 2 };
        }
    }
}
struct ScriptCamera {
    request: warzone_app_client::battle_camera::Request,
    from: [f64; 2],
    started: std::time::Instant,
    pvp: bool,
}
impl ScriptCamera {
    fn target(&self, projection: &warzone_app_client::Projection, camera: &Camera) -> [f64; 2] {
        if self.pvp {
            let p = projection.project(self.request.point[0] as f64, self.request.point[1] as f64);
            [
                p[0],
                p[1] - (camera.height as f64 * 0.5 - 50. * warzone_app_client::hud::scale(camera))
                    / camera.zoom,
            ]
        } else {
            self.request.target(projection, camera)
        }
    }
}
pub struct Ui {
    pub now: i64,
    rewards: warzone_app_client::reward_effects::Playback,
    reward_clock: std::time::Instant,
    story_motion: warzone_app_client::interface_story::Playback,
    camera_serial: u64,
    camera_motion: Option<ScriptCamera>,
    pub camera: Camera,
    pub view: Scene,
    input: crate::Input,
    selection: warzone_app_client::battle_selection::Selection,
    revision: u64,
    battle_action_revision: u64,
    unit_index: usize,
    objectives: bool,
    surrender: bool,
    pause_confirm: bool,
    pressed: Option<u8>,
    source_pressed: Option<SourceAction>,
    card_gesture: Option<CardGesture>,
    placement_drag: Option<(String, [i32; 2], [i32; 2])>,
    card_return: Option<(u32, [i32; 2], [i32; 2], std::time::Instant)>,
    focus_step: Option<usize>,
    focus_at: f64,
    list_scroll: f64,
    result_scroll: [f64; 3],
    result_drag: Option<(usize, i32, f64)>,
    result_velocity: [f64; 3],
    result_clock: std::time::Instant,
    result_sample: Option<(i32, std::time::Instant)>,
    list_drag: Option<(i32, f64)>,
    pub notice: Option<String>,
    energy_refill: bool,
    sound_cursor: usize,
    playback: Option<(
        warzone_app_client::battle_motion::Playback,
        std::time::Instant,
    )>,
}
fn fit_landing(camera: &mut Camera, scene: &Scene, state: &State) {
    // Source SceneBattle positions the landing area; it does not shrink it until
    // every cell fits. Keep the early preview rule only for legacy asset packs.
    if warzone_app_client::interface_battle::available(scene) {
        return;
    }
    let Some(s) = &state.mission else {
        return;
    };
    let width = camera.width;
    let height = camera.height;
    if s.battle.phase == "placement" {
        if let Some(zone) = scene
            .combat
            .as_ref()
            .and_then(|c| c.missions.get(&s.battle.mission))
            .and_then(|m| m.zones.iter().find(|z| z.kind == 2))
        {
            let x = zone.x as f64 - 0.5;
            let y = zone.y as f64 - 0.5;
            let w = zone.w as f64;
            let h = zone.h as f64;
            let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
            let bounds = |cam: &Camera| {
                let p = corners.map(|p| cam.screen(scene.projection.project(p[0], p[1])));
                [
                    p.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
                    p.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
                    p.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max),
                    p.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max),
                ]
            };
            let top = warzone_app_client::hud::height(scene, camera) as f64 + 20.;
            let bottom = height as f64 - 158.;
            let b = bounds(camera);
            let fit = ((width as f64 - 32.) / (b[2] - b[0]))
                .min((bottom - top) / (b[3] - b[1]))
                .min(1.);
            let density = warzone_app_client::browser::zoom_factor();
            camera.zoom = (camera.zoom * fit).clamp(0.12 * density, 2. * density);
            let b = bounds(camera);
            let dx = if b[0] < 16. {
                16. - b[0]
            } else if b[2] > width as f64 - 16. {
                width as f64 - 16. - b[2]
            } else {
                0.
            };
            let dy = if b[1] < top {
                top - b[1]
            } else if b[3] > bottom {
                bottom - b[3]
            } else {
                0.
            };
            camera.pan(dx, dy);
        }
    }
}
fn reposition_cell(
    scene: &Scene,
    b: &warzone_app_client::battle::Battle,
    camera: &Camera,
    uid: &str,
    start: [i32; 2],
    point: [i32; 2],
) -> Option<[i32; 2]> {
    let u = &b.units[b.index(uid)?];
    let a = camera.world(start.map(|p| p as f64));
    let z = camera.world(point.map(|p| p as f64));
    let a = scene.projection.inverse(a[0], a[1]);
    let z = scene.projection.inverse(z[0], z[1]);
    // An existing world unit keeps its grab offset. The25pt roster-card
    // descent offset belongs only to CHUDBattle.putSoldier, not this drag.
    Some([
        (u.x - 16. + z[0] - a[0] + 0.5).floor() as i32,
        (u.y - 16. + z[1] - a[1] + 0.5).floor() as i32,
    ])
}
impl Ui {
    fn battle_action_revision(state: &State) -> u64 {
        let Some(session) = &state.mission else {
            return 0;
        };
        state
            .actions
            .iter()
            .rev()
            .find(|a| {
                a.uid == session.id
                    && matches!(
                        a.kind.as_str(),
                        "mission_action" | "mission_win" | "mission_defeat"
                    )
            })
            .map_or(0, |a| a.sequence)
    }
    pub fn reward_hud_values(
        &self,
        scene: &Scene,
        state: &State,
        wall: i64,
    ) -> warzone_app_client::hud::Values {
        self.rewards.hud_values(scene, state, wall)
    }

    pub fn show_energy_refill(&mut self, scene: &Scene, state: &State) -> bool {
        if warzone_app_client::interface_energy::available(scene)
            && state.energy(scene, self.now) < state.max_energy(scene)
        {
            self.energy_refill = true;
            self.notice = None;
            self.input.drag = None;
            true
        } else {
            false
        }
    }
    pub fn update_clock(&mut self, scene: &Scene, state: &State, now: i64) {
        self.now = now;
        if self.energy_refill && state.energy(scene, now) >= state.max_energy(scene) {
            self.energy_refill = false;
            self.source_pressed = None;
        }
    }
    pub fn take_reward_sounds(&mut self) -> Vec<&'static str> {
        self.rewards.take_sounds()
    }
    pub fn take_sounds(&mut self) -> Vec<warzone_app_client::battle_motion::SoundCue> {
        let Some((playback, clock)) = &mut self.playback else {
            return vec![];
        };
        let elapsed = clock.elapsed().as_secs_f64();
        let result = playback
            .sounds
            .iter()
            .filter(|s| s.at <= elapsed)
            .cloned()
            .collect::<Vec<_>>();
        playback.sounds.retain(|s| s.at > elapsed);
        self.sound_cursor += result.len();
        result
    }
    pub fn selected_type(&self) -> Option<u32> {
        self.input
            .selected
            .and_then(|i| self.view.entities.get(i))
            .filter(|e| e.kind == "unit" && e.archived["enemy"] != true)
            .map(|e| e.type_id)
    }
    fn pick_current(
        &self,
        screen: [f64; 2],
        accept: impl Fn(&warzone_app_client::Entity) -> bool,
    ) -> Option<usize> {
        let motion = self
            .playback
            .as_ref()
            .map(|(p, t)| (p, t.elapsed().as_secs_f64()));
        self.view
            .pick_battle_filtered_positions(&self.camera, screen, accept, |e| {
                motion
                    .and_then(|(p, t)| p.sample(&e.uid, t))
                    .map_or([e.x, e.y], |p| [p.x, p.y])
            })
    }
    pub fn motion_report(&self) -> Option<serde_json::Value> {
        self.playback
            .as_ref()
            .map(|(p, t)| p.snapshot(t.elapsed().as_secs_f64()))
    }
    pub fn script_pointer_blocked(&self, scene: &Scene, state: &State) -> bool {
        (self.camera_motion.is_some()
            || state
                .mission
                .as_ref()
                .and_then(|m| m.battle.camera_request.as_ref())
                .is_some_and(|r| r.serial > self.camera_serial))
            && warzone_app_client::story::current(scene, state).is_none()
    }
    fn diagnostic_pick_point(
        &self,
        scene: &Scene,
        state: &State,
        uid: &str,
    ) -> Result<Option<[i32; 2]>, String> {
        let e = self
            .view
            .entities
            .iter()
            .find(|e| e.uid == uid)
            .ok_or("Missing visible replay entity")?;
        let source = self.source_view(scene, state);
        let [x, y, w, h] = e.footprint();
        let mut cells = vec![[e.x, e.y]];
        for i in 0..=(w * 2.) as i32 {
            for j in 0..=(h * 2.) as i32 {
                cells.push([x - 0.25 + i as f64 * 0.5, y - 0.25 + j as f64 * 0.5]);
            }
        }
        let mut bonus = false;
        for p in cells {
            let p = self.camera.screen(scene.projection.project(p[0], p[1]));
            let q = [(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32];
            if q[0] < 0
                || q[1] < warzone_app_client::hud::height(scene, &self.camera)
                || q[0] >= self.camera.width as i32
                || q[1] >= self.camera.height as i32
            {
                continue;
            }
            if !self
                .pick_current([q[0] as f64, q[1] as f64], |_| true)
                .is_some_and(|i| self.view.entities[i].uid == uid)
            {
                continue;
            }
            if let Some(v) = &source {
                if v.blocks(q[0], q[1]) {
                    continue;
                }
                if let Some(action) = v.hit(q[0], q[1]) {
                    bonus |= matches!(action, SourceAction::FallingPickup(..));
                    continue;
                }
            }
            return Ok(Some(q));
        }
        // The source bonus owns its touch rectangle until manual/automatic
        // flight. A test player may wait; never click through it or consume a
        // paid battle command against a different selection.
        if bonus {
            return Ok(None);
        }
        let screen = self.camera.screen(scene.projection.project(e.x, e.y));
        Err(format!(
            "No selectable visible footprint for {uid}; entity={:?}; screen={screen:?}; camera={:?}; story={:?}; pan={}; serial={}; modal={}; notice={:?}",
            [e.x, e.y],
            [self.camera.x, self.camera.y, self.camera.zoom],
            warzone_app_client::story::current(scene, state).map(|s| s.0),
            self.camera_motion.is_some(),
            self.camera_serial,
            source.as_ref().is_some_and(|v| v.modal),
            self.notice
        ))
    }
    pub fn script_bonus_obstructs(
        &self,
        scene: &Scene,
        state: &State,
        action: &serde_json::Value,
    ) -> bool {
        if !matches!(
            action[0].as_str(),
            Some("move" | "move_confirm" | "attack" | "attack_confirm")
        ) {
            return false;
        }
        action.as_array().is_some_and(|a| {
            a.iter().skip(1).filter_map(|v| v.as_str()).any(|uid| {
                self.diagnostic_pick_point(scene, state, uid)
                    .is_ok_and(|p| p.is_none())
            })
        })
    }
    pub fn script_visuals_pending(&self) -> bool {
        self.motion_active() || self.camera_motion.is_some()
    }
    fn diagnostic_drag_start(&self, scene: &Scene, state: &State) -> Result<[i32; 2], String> {
        let view = self.source_view(scene, state);
        diagnostic_map_drag_start(
            &self.camera,
            warzone_app_client::hud::height(scene, &self.camera),
            view.as_ref(),
        )
        .ok_or_else(|| "No unblocked tactical point for diagnostic camera drag".into())
    }
    pub fn story_input_ready(&self, scene: &Scene, state: &State) -> bool {
        !(self.motion_active() && warzone_app_client::story::current(scene, state).is_some())
            && self
                .story_motion
                .input_ready(self.reward_clock.elapsed().as_secs_f64())
    }
    pub fn new(scene: &Scene, state: &State, width: u32, height: u32) -> Result<Self, String> {
        let s = state.mission.as_ref().ok_or("Missing native mission")?;
        let mut camera = Camera {
            x: s.camera[0],
            y: s.camera[1],
            zoom: s.camera[2] * warzone_app_client::browser::zoom_factor(),
            width,
            height,
        };
        let prepared = scene
            .combat
            .as_ref()
            .and_then(|c| c.views.get(&s.battle.mission));
        if warzone_app_client::interface_battle::available(scene)
            && s.battle.tutorial.is_none()
            && s.battle.phase == "placement"
            && state
                .actions
                .last()
                .is_some_and(|a| a.kind == "mission_open")
            && prepared.is_some_and(|v| v.camera == s.camera)
        {
            if let Some(zone) = scene
                .combat
                .as_ref()
                .and_then(|c| c.missions.get(&s.battle.mission))
                .and_then(|m| m.zones.iter().find(|z| z.kind == 2))
            {
                // SceneBattle.init -> scrollToPlaceX:y:onCenter: (0x6584d):
                // first descent area's x+w/2, y-4; phone SP scale .5 and
                // vertical anchor screenHeight/5. Source Draw.coord2Screen2
                // uses the same projection as our scene. ARM at 960x640 has
                // world root (176,-231)*2, equivalent to (128,782,1) here.
                let focus = scene
                    .projection
                    .project((zone.x + zone.w / 2) as f64, (zone.y - 4) as f64);
                camera.zoom = warzone_app_client::hud::scale(&camera) * 0.5;
                camera.x = focus[0].trunc();
                camera.y = focus[1].trunc() + height as f64 * 0.3 / camera.zoom;
            }
        }
        if s.battle.tutorial.is_some() && s.camera_serial == 0 && s.battle.camera_request.is_none()
        {
            // Accepted original5000initial FBO: root(200.4,-1730), SPzoom.6
            // at960x640, giving this source world center. Do not use browser
            // preview camera(748,3302,.8) from the historical prepared assets.
            camera.x = 66.;
            camera.y = 3150.;
            camera.zoom = warzone_app_client::hud::scale(&camera) * 0.6;
        }
        fit_landing(&mut camera, scene, state);
        let mut result = Self {
            now: state.last_action_time,
            rewards: Default::default(),
            reward_clock: std::time::Instant::now(),
            story_motion: Default::default(),
            camera_serial: s.camera_serial,
            camera_motion: None,
            camera,
            view: state.mission_scene(scene)?,
            input: crate::Input::default(),
            selection: Default::default(),
            revision: state.revision,
            battle_action_revision: Self::battle_action_revision(state),
            unit_index: 0,
            objectives: false,
            surrender: false,
            pause_confirm: false,
            pressed: None,
            playback: None,
            sound_cursor: 0,
            source_pressed: None,
            card_gesture: None,
            placement_drag: None,
            card_return: None,
            focus_step: s.battle.tutorial.as_ref().map(|t| t.index),
            focus_at: 0.,
            list_scroll: 0.,
            result_scroll: [0.; 3],
            result_drag: None,
            result_velocity: [0.; 3],
            result_clock: std::time::Instant::now(),
            result_sample: None,
            list_drag: None,
            notice: None,
            energy_refill: false,
        };
        result.rewards.initialize(state.revision);
        result.admit_camera(scene, state);
        Ok(result)
    }
    fn motion_active(&self) -> bool {
        self.playback
            .as_ref()
            .is_some_and(|(p, t)| t.elapsed().as_secs_f64() < p.duration)
    }
    pub fn result_presentation_ready(&self) -> bool {
        !self.motion_active()
    }
    fn admit_camera(&mut self, scene: &Scene, state: &State) {
        // The durable trigger may already have queued the next tutorial pan.
        // Present it only after the committed attack/movement has finished.
        if self.motion_active() {
            return;
        }
        let Some(r) = state
            .mission
            .as_ref()
            .and_then(|s| s.battle.camera_request.as_ref())
            .filter(|r| r.serial > self.camera_serial)
        else {
            return;
        };
        self.camera_serial = r.serial;
        self.input.selected = None;
        self.input.drag = None;
        self.camera_motion = Some(ScriptCamera {
            request: r.clone(),
            from: [self.camera.x, self.camera.y],
            started: std::time::Instant::now(),
            pvp: state
                .mission
                .as_ref()
                .is_some_and(|s| s.battle.pvp.is_some()),
        });
        self.advance_camera(scene);
    }
    fn advance_camera(&mut self, scene: &Scene) {
        if let Some(m) = &self.camera_motion {
            let to = m.target(&scene.projection, &self.camera);
            let (p, done) = m
                .request
                .pose(m.from, to, m.started.elapsed().as_secs_f64());
            self.camera.x = p[0];
            self.camera.y = p[1];
            if done {
                self.camera_motion = None;
            }
        }
    }
    pub fn sync(&mut self, scene: &Scene, state: &State) -> Result<(), String> {
        let elapsed = self.result_clock.elapsed().as_secs_f64();
        self.result_clock = std::time::Instant::now();
        if self.result_drag.is_none()
            && state
                .mission
                .as_ref()
                .is_some_and(|m| m.battle.result.is_some())
        {
            let s = warzone_app_client::hud::scale(&self.camera);
            for (row, items) in warzone_app_client::interface_result::rows(state)
                .iter()
                .enumerate()
            {
                let max = (items.len() as f64 * 44. + 30. - 238.).max(0.) * s;
                self.result_scroll[row] = warzone_app_client::scroll_motion::coast(
                    self.result_scroll[row],
                    &mut self.result_velocity[row],
                    max,
                    elapsed,
                );
            }
        }
        let step = state
            .mission
            .as_ref()
            .and_then(|m| m.battle.tutorial.as_ref())
            .map(|t| t.index);
        if step != self.focus_step {
            self.focus_step = step;
            self.focus_at = self.reward_clock.elapsed().as_secs_f64();
        }
        if self.playback.is_some() && !self.motion_active() {
            self.playback = None;
        }
        if self.revision == state.revision {
            self.admit_camera(scene, state);
        }
        self.advance_camera(scene);
        if self.revision != state.revision {
            let selected = self
                .input
                .selected
                .and_then(|i| self.view.entities.get(i))
                .map(|e| e.uid.clone());
            let next = state.mission_scene(scene)?;
            let battle_action_revision = Self::battle_action_revision(state);
            if let Some(c) = scene.combat.as_ref().filter(|c| {
                !c.animations.is_empty() && battle_action_revision != self.battle_action_revision
            }) {
                let events = &state.mission.as_ref().unwrap().battle.visuals;
                if let Some((playback, clock)) = &mut self.playback {
                    playback.append(&self.view, &next, c, events, clock.elapsed().as_secs_f64());
                } else {
                    let playback = warzone_app_client::battle_motion::Playback::new(
                        &self.view, &next, c, events,
                    );
                    self.sound_cursor = 0;
                    self.playback =
                        (playback.duration > 0.).then_some((playback, std::time::Instant::now()));
                }
            }
            if battle_action_revision != self.battle_action_revision {
                self.selection
                    .reconcile(&state.mission.as_ref().unwrap().battle);
            }
            self.battle_action_revision = battle_action_revision;
            self.view = next;
            self.input.selected =
                selected.and_then(|uid| self.view.entities.iter().position(|e| e.uid == uid));
            self.revision = state.revision;
            self.admit_camera(scene, state);
            if let Some(t) = state
                .mission
                .as_ref()
                .and_then(|s| s.battle.tutorial.as_ref())
            {
                self.input.selected = t
                    .selected
                    .as_ref()
                    .and_then(|uid| self.view.entities.iter().position(|e| &e.uid == uid));
            }
        }
        self.rewards
            .update(scene, state, self.reward_clock.elapsed().as_secs_f64());
        self.rewards.prepare_camera(scene, &self.camera);
        self.story_motion.observe(
            scene,
            state,
            !self.motion_active(),
            self.reward_clock.elapsed().as_secs_f64(),
        );
        Ok(())
    }
    pub fn source_view(&self, scene: &Scene, state: &State) -> Option<SourceView> {
        let types = Self::types(scene, state);
        let ghost = self
            .card_gesture
            .as_ref()
            .filter(|g| g.mode == 2)
            .and_then(|g| types.get(g.index).map(|id| (*id, g.point)));
        let mut v = warzone_app_client::interface_battle::build(
            scene,
            state,
            &self.camera,
            self.list_scroll,
            ghost,
        )?;
        if let Some((id, from, to, started)) = &self.card_return {
            let t = started.elapsed().as_secs_f64() / 0.2;
            if t < 1. {
                let t = warzone_app_client::interface_story::ease(t);
                let point = [
                    (from[0] as f64 + (to[0] - from[0]) as f64 * t).round() as i32,
                    (from[1] as f64 + (to[1] - from[1]) as f64 * t).round() as i32,
                ];
                warzone_app_client::interface_battle::return_thumbnail(
                    &mut v,
                    scene,
                    state,
                    &self.camera,
                    *id,
                    point,
                );
            }
        }
        if state
            .mission
            .as_ref()
            .is_some_and(|m| m.battle.tutorial.is_none() && m.battle.result.is_none())
        {
            warzone_app_client::interface_energy::hud(&mut v, scene, &self.camera);
        }
        warzone_app_client::interface_pvp::timer(&mut v, scene, state, &self.camera, self.now);
        let active = self.motion_active();
        if warzone_app_client::interface_unit_health::available(scene) {
            v.retained_unit_health = true;
            let motion = self
                .playback
                .as_ref()
                .map(|(p, t)| (p, t.elapsed().as_secs_f64()));
            let ghosts = motion.map_or(&[][..], |(p, _)| p.ghosts.as_slice());
            let b = &state.mission.as_ref()?.battle;
            let source_selection = scene.combat.as_ref()?.selection.is_some();
            let target = source_selection
                .then(|| {
                    b.tutorial
                        .as_ref()
                        .and_then(|t| t.enemy.as_ref())
                        .or(self.selection.target.as_ref())
                })
                .flatten();
            for entity in self.view.entities.iter().chain(ghosts).filter(|e| {
                e.kind == "unit"
                    || source_selection && matches!(e.kind.as_str(), "building" | "decor")
            }) {
                let hit = motion.is_some_and(|(p, t)| {
                    p.hits
                        .iter()
                        .any(|h| h.target == entity.uid && h.start <= t && t < h.attack_end)
                });
                let pose = motion.and_then(|(p, t)| p.sample(&entity.uid, t));
                let selected = target.is_none()
                    && self
                        .input
                        .selected
                        .and_then(|i| self.view.entities.get(i))
                        .is_some_and(|e| e.uid == entity.uid);
                let selected = (selected || target == Some(&entity.uid))
                    && !hit
                    && !pose.as_ref().is_some_and(|p| p.pose != "idle");
                if !hit && !selected {
                    continue;
                }
                if pose.as_ref().is_some_and(|p| p.finished_death) {
                    continue;
                }
                let (x, y, hp) = pose.as_ref().map_or(
                    (
                        entity.x,
                        entity.y,
                        entity.archived["hp"].as_f64().unwrap_or(0.),
                    ),
                    |p| (p.x, p.y, p.hp),
                );
                let max = entity.archived["hp_max"].as_f64().unwrap_or(1.);
                let position = self.camera.screen(scene.projection.project(x, y));
                let health_start = v.draws.len();
                if selected && scene.combat.as_ref().is_some_and(|c| c.selection.is_some()) {
                    if let Some(u) = b.index(&entity.uid).map(|i| &b.units[i]) {
                        warzone_app_client::interface_unit_health::full(
                            &mut v,
                            scene,
                            &self.camera,
                            position,
                            u,
                            hp,
                        );
                    }
                } else if matches!(entity.kind.as_str(), "building" | "decor")
                    && !matches!(entity.type_id, 1021 | 1022 | 1036)
                    && scene.combat.as_ref().is_some_and(|c| c.selection.is_some())
                {
                    if let Some(u) = b.index(&entity.uid).map(|i| &b.units[i]) {
                        warzone_app_client::interface_unit_health::building(
                            &mut v,
                            scene,
                            &self.camera,
                            position,
                            u,
                            hp,
                        );
                    }
                } else {
                    warzone_app_client::interface_unit_health::add(
                        &mut v,
                        scene,
                        &self.camera,
                        position,
                        hp,
                        max,
                    );
                }
                for d in &mut v.draws[health_start..] {
                    d.node
                        .parents
                        .push(format!("source_unit_uid_{}", entity.uid));
                }
            }
        }
        if !active || state.mission.as_ref()?.battle.tutorial.is_none() {
            let b = &state.mission.as_ref()?.battle;
            if let Some(t) = &b.tutorial {
                let mut selection = warzone_app_client::battle_selection::Selection::default();
                selection.unit = t.selected.clone();
                selection.target = t.enemy.clone();
                if let (Some(uid), Some(p)) = (&t.selected, t.route) {
                    selection.route = warzone_app_client::battle_selection::route(
                        b,
                        scene.combat.as_ref()?,
                        uid,
                        p,
                    )
                    .unwrap_or_default();
                }
                warzone_app_client::battle_selection::markers(
                    &mut v,
                    scene,
                    b,
                    &self.camera,
                    &selection,
                    self.reward_clock.elapsed().as_secs_f64(),
                );
            } else {
                let mut preview = None;
                if let Some((uid, start, point)) = &self.placement_drag {
                    if let Some(p) = reposition_cell(scene, b, &self.camera, uid, *start, *point) {
                        let mut moved = b.clone();
                        let valid = moved.reposition(scene.combat.as_ref()?, uid, p)
                            || b.index(uid)
                                .is_some_and(|i| b.units[i].cell() == [p[0] + 16, p[1] + 16]);
                        if let Some(i) = moved.index(uid) {
                            moved.units[i].x = (p[0] + 16) as f64;
                            moved.units[i].y = (p[1] + 16) as f64;
                        }
                        if (point[0] - start[0]).abs() + (point[1] - start[1]).abs()
                            > crate::input_events::slop(6)
                        {
                            let (x, y) = (p[0] as f64, p[1] as f64);
                            v.deployment_preview = Some((
                                [[x, y], [x + 1., y], [x + 1., y + 1.], [x, y + 1.]].map(|p| {
                                    self.camera.screen(scene.projection.project(p[0], p[1]))
                                }),
                                valid,
                            ));
                        }
                        preview = Some(moved);
                    }
                }
                warzone_app_client::battle_selection::markers(
                    &mut v,
                    scene,
                    preview.as_ref().unwrap_or(b),
                    &self.camera,
                    &self.selection,
                    self.reward_clock.elapsed().as_secs_f64(),
                );
            }
        } else {
            v.retained_battle_selection = scene.combat.as_ref()?.selection.is_some();
        }
        self.rewards.draw(
            &mut v,
            scene,
            state,
            &self.camera,
            self.reward_clock.elapsed().as_secs_f64(),
        );
        if self.objectives {
            warzone_app_client::interface_battle::goals(&mut v, scene, state, &self.camera);
        }
        if self.pause_confirm || self.surrender {
            let key = if self.pause_confirm {
                "#pause_battle"
            } else {
                if state
                    .mission
                    .as_ref()
                    .is_some_and(|s| s.battle.pvp.is_some())
                {
                    "#dialog_fail_retreat_pvp"
                } else {
                    "#dialog_fail_retreat"
                }
            };
            let text = scene
                .progression
                .as_ref()
                .and_then(|c| c.language.get(key))
                .map(String::as_str)
                .unwrap_or("");
            warzone_app_client::interface_windows::message(&mut v, scene, &self.camera, text, true);
        }
        if self.energy_refill {
            warzone_app_client::interface_energy::build(
                &mut v,
                scene,
                state,
                &self.camera,
                self.now,
            );
        }
        if let Some(text) = &self.notice {
            let mut popup = SourceView::default();
            warzone_app_client::interface_windows::message(
                &mut popup,
                scene,
                &self.camera,
                text,
                false,
            );
            v.draws.extend(popup.draws);
            v.hits = popup.hits;
            v.blocks.clear();
            v.modal = true;
        }
        if !self.motion_active() && self.notice.is_none() && !self.energy_refill {
            warzone_app_client::interface_result::add(
                &mut v,
                scene,
                state,
                &self.camera,
                self.result_scroll,
            );
            warzone_app_client::interface_story::add(&mut v, scene, state, &self.camera);
            warzone_app_client::interface_story::focus_animated(
                &mut v,
                scene,
                state,
                &self.camera,
                (self.reward_clock.elapsed().as_secs_f64() - self.focus_at).max(0.),
            );
            self.story_motion.apply(
                &mut v,
                &self.camera,
                self.reward_clock.elapsed().as_secs_f64(),
            );
        }
        // CHUDBattle adds its drag UIImage directly to nativeView after the
        // personage dialog already exists. Keep that captured image on top.
        let mut ghosts = Vec::new();
        v.draws.retain(|d| {
            if d.node.id == "deployment_drag" {
                ghosts.push(d.clone());
                false
            } else {
                true
            }
        });
        v.draws.extend(ghosts);
        Some(v)
    }
    pub fn source_report(&self, scene: &Scene, state: &State) -> Option<serde_json::Value> {
        let v = self.source_view(scene, state)?;
        Some(
            serde_json::json!({"source_geometry":true,"camera":[self.camera.x,self.camera.y,self.camera.zoom],"script_camera_active":self.camera_motion.is_some(),"presentation_pending":self.motion_active(),"script_camera_serial":self.camera_serial,"result":state.mission.as_ref()?.battle.result,"story_key":warzone_app_client::story::current(scene,state).map(|p|p.0),"placement":state.mission.as_ref()?.battle.phase=="placement",
            "deployment_preview_valid":v.deployment_preview.map(|(_,valid)|valid),"result_scroll":self.result_scroll,"energy_refill":self.energy_refill,"objectives":self.objectives,"pause_confirm":self.pause_confirm,"surrender":self.surrender,"notice":self.notice,"scroll":self.list_scroll,"scroll_max":v.scroll_max,
            "selected_type":self.unit_type(scene,state),"selection":{"unit":self.selection.unit,"target":self.selection.target,"route":self.selection.route},"source_markers":v.retained_battle_selection,"draws":v.draws.len(),
            "unit_counts":v.draws.iter().filter(|d|d.node.id=="unit_counter").map(|d|d.node.text.clone()).collect::<Vec<_>>(),
            "bindings":v.draws.iter().filter(|d|["header_txt","hint_txt","goal1_txt","goal2_txt","goal3_txt","battlestart_txt","unit_name"].contains(&d.node.id.as_str())).map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text})).collect::<Vec<_>>(),
            "hit_targets":v.hits.iter().map(|h|serde_json::json!({"action":format!("{:?}",h.action),"rect":h.rect})).collect::<Vec<_>>() }),
        )
    }
    fn source_action(&mut self, a: SourceAction, scene: &Scene, state: &State) -> Option<Command> {
        if self.notice.is_some() {
            if matches!(
                a,
                SourceAction::Dismiss
                    | SourceAction::Close
                    | SourceAction::Confirm
                    | SourceAction::Primary
                    | SourceAction::Cancel
            ) {
                self.notice = None;
                self.card_gesture = None;
                self.source_pressed = None;
                self.input.drag = None;
            }
            return None;
        }
        if let SourceAction::FallingPickup(sequence, id, instance) = a {
            self.rewards.pickup(
                sequence,
                id,
                instance,
                self.reward_clock.elapsed().as_secs_f64(),
            );
            return None;
        }
        if warzone_app_client::story::current(scene, state).is_some()
            && state
                .mission
                .as_ref()
                .and_then(|s| s.battle.tutorial_step(scene.combat.as_ref()?))
                .is_none_or(|s| !s.button.is_empty())
        {
            return (a == SourceAction::Confirm).then_some(Self::dialogue_command(state));
        }
        if state
            .mission
            .as_ref()
            .is_some_and(|s| s.battle.result.is_some())
        {
            return matches!(a, SourceAction::Primary | SourceAction::Close)
                .then_some(Command::Return);
        }
        match a {
            SourceAction::Energy => {
                if state.energy(scene, self.now) >= state.max_energy(scene) {
                    self.notice = Some(warzone_app_client::interface_energy::full_message(scene));
                } else if warzone_app_client::interface_energy::available(scene) {
                    self.energy_refill = true;
                    self.input.drag = None;
                    self.card_gesture = None;
                    self.list_drag = None;
                }
                None
            }
            SourceAction::EnergyBuy => {
                if self.energy_refill {
                    Some(Command::RefillEnergy)
                } else {
                    None
                }
            }
            SourceAction::EnergyWait => {
                self.energy_refill = false;
                None
            }
            SourceAction::Dismiss => {
                self.notice = None;
                self.pause_confirm = false;
                self.surrender = false;
                None
            }
            SourceAction::Confirm => {
                if self.pause_confirm {
                    self.pause_confirm = false;
                    Some(Command::Suspend)
                } else if self.surrender {
                    self.surrender = false;
                    Some(
                        if scene
                            .interface
                            .as_ref()
                            .is_some_and(|c| c.symbols.contains_key("battle_result"))
                        {
                            Command::Retreat
                        } else {
                            Command::Surrender
                        },
                    )
                } else {
                    None
                }
            }
            SourceAction::Cancel => {
                if let Some(b) = state
                    .mission
                    .as_ref()
                    .map(|s| &s.battle)
                    .filter(|b| b.pvp_score().is_some_and(|s| s.stars > 0))
                {
                    return b
                        .pvp
                        .as_ref()
                        .is_some_and(|p| p.finish_hint_acknowledged)
                        .then_some(Command::FinishPvp);
                }
                self.surrender = true;
                None
            }
            SourceAction::Close => {
                if self.notice.take().is_some() {
                    return None;
                }
                if self.energy_refill {
                    self.energy_refill = false;
                    return None;
                }
                if self.objectives {
                    self.objectives = false;
                    None
                } else if self.pause_confirm || self.surrender {
                    self.pause_confirm = false;
                    self.surrender = false;
                    None
                } else if state.mission.as_ref().unwrap().battle.phase == "placement" {
                    Some(Command::CancelPlacement)
                } else {
                    self.pause_confirm = true;
                    None
                }
            }
            SourceAction::Primary => {
                self.card_gesture = None;
                self.list_drag = None;
                self.source_pressed = None;
                let b = &state.mission.as_ref().unwrap().battle;
                if b.phase == "placement" && !b.units.iter().any(|u| u.profile.is_some()) {
                    self.notice = Some(
                        scene
                            .progression
                            .as_ref()
                            .and_then(|c| c.language.get("#battlefield_no_units"))
                            .cloned()
                            .unwrap_or_else(|| "Высади отряды на поле боя".into()),
                    );
                    None
                } else {
                    Some(Self::primary(state))
                }
            }
            SourceAction::Quest(_) => {
                self.card_gesture = None;
                self.list_drag = None;
                self.input.drag = None;
                self.objectives = true;
                None
            }
            SourceAction::Choose(index) => {
                self.unit_index = index;
                None
            }
            _ => None,
        }
    }
    fn primary(state: &State) -> Command {
        let b = &state.mission.as_ref().unwrap().battle;
        if b.result.is_some() {
            Command::Return
        } else if b.phase == "placement" {
            Command::Start
        } else {
            Command::Skip
        }
    }
    fn cycle(&mut self, state: &State) {
        let b = &state.mission.as_ref().unwrap().battle;
        if b.phase == "placement" {
            self.unit_index = self.unit_index.wrapping_add(1);
        } else {
            let ids: Vec<_> = self
                .view
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| b.units.iter().any(|u| u.uid == e.uid && u.own && u.hp > 0))
                .map(|(i, _)| i)
                .collect();
            if !ids.is_empty() {
                let i = self
                    .input
                    .selected
                    .and_then(|s| ids.iter().position(|i| *i == s))
                    .map_or(0, |i| (i + 1) % ids.len());
                self.input.selected = Some(ids[i]);
                self.selection.unit = Some(self.view.entities[ids[i]].uid.clone());
                self.selection.clear_pending();
                let e = &self.view.entities[ids[i]];
                let p = self.view.projection.project(e.x, e.y);
                self.camera.x = p[0];
                self.camera.y = p[1];
            }
        }
    }
    fn types(_scene: &Scene, state: &State) -> Vec<u32> {
        warzone_app_client::battle_selection::types(&state.mission.as_ref().unwrap().battle)
    }
    fn unit_type(&self, scene: &Scene, state: &State) -> u32 {
        let types = Self::types(scene, state);
        types
            .get(self.unit_index % types.len().max(1))
            .copied()
            .unwrap_or(2001)
    }
    fn hit(&self, scene: &Scene, x: i32, y: i32) -> Option<u8> {
        let p = Point::new(x, y);
        if let Some(i) = warzone_app_client::hud::navigation(scene, &self.camera)
            .iter()
            .position(|r| {
                let [x, y, w, h] = *r;
                Rect::new(x, y, w as u32, h as u32).contains_point(p)
            })
        {
            return Some(i as u8);
        }
        if Rect::new(16, self.camera.height as i32 - 44, 170, 36).contains_point(p) {
            Some(0)
        } else if Rect::new(200, self.camera.height as i32 - 44, 230, 36).contains_point(p) {
            Some(1)
        } else {
            None
        }
    }
    fn dialogue_command(state: &State) -> Command {
        if state
            .mission
            .as_ref()
            .is_some_and(|s| s.battle.pvp.is_some())
        {
            Command::PvpDialogue
        } else if state
            .mission
            .as_ref()
            .is_some_and(|s| s.battle.tutorial.is_some())
        {
            Command::Tutorial(warzone_app_client::battle::TutorialCommand::Okay)
        } else {
            Command::AdvanceDialogue
        }
    }
    fn modal(&self, state: &State) -> bool {
        self.objectives || self.surrender || state.mission.as_ref().unwrap().battle.result.is_some()
    }
    pub fn event(
        &mut self,
        event: Event,
        scene: &Scene,
        state: &State,
    ) -> Result<Option<Command>, String> {
        self.update_clock(scene, state, self.now);
        self.sync(scene, state)?;
        // SceneBattle's _scrollActive suppresses map input during trigger pans;
        // personage text can still own the click above that scene layer.
        if self.camera_motion.is_some()
            && warzone_app_client::story::current(scene, state).is_none()
            && matches!(
                event,
                Event::MouseButtonDown { .. }
                    | Event::MouseButtonUp { .. }
                    | Event::MouseMotion { .. }
                    | Event::MouseWheel { .. }
            )
        {
            return Ok(None);
        }
        // Preserve the queued animation. Map pan/zoom stays responsive, but
        // another action must wait instead of teleporting every actor to its
        // durable final position (and dropping enemy fire cues).
        if self.motion_active()
            && state.mission.as_ref().is_some_and(|m| {
                m.battle.tutorial.is_some()
                    || m.battle.result.is_some()
                    || warzone_app_client::story::current(scene, state).is_some()
            })
        {
            self.input.event(event, &mut self.camera, &self.view);
            return Ok(None);
        }
        if matches!(
            event,
            Event::Window {
                win_event: sdl2::event::WindowEvent::SizeChanged(_, _),
                ..
            }
        ) {
            self.card_gesture = None;
            self.source_pressed = None;
            self.list_drag = None;
            self.input.event(event, &mut self.camera, &self.view);
            fit_landing(&mut self.camera, scene, state);
            return Ok(None);
        }
        let b = &state.mission.as_ref().unwrap().battle;
        if b.phase == "placement" && b.tutorial.is_none() {
            if let Some((uid, start, point)) = &mut self.placement_drag {
                match event {
                    Event::MouseMotion { x, y, .. } => {
                        *point = [x, y];
                        if let Some(p) =
                            reposition_cell(scene, b, &self.camera, uid, *start, *point)
                        {
                            if let Some(e) = self.view.entities.iter_mut().find(|e| e.uid == *uid) {
                                e.x = p[0] as f64;
                                e.y = p[1] as f64;
                            }
                        }
                        return Ok(None);
                    }
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        let uid = uid.clone();
                        let moved = (x - start[0]).abs() + (y - start[1]).abs()
                            > crate::input_events::slop(6);
                        let p = reposition_cell(scene, b, &self.camera, &uid, *start, [x, y]);
                        self.placement_drag = None;
                        self.input.drag = None;
                        self.view = state.mission_scene(scene)?;
                        self.input.selected = self.view.entities.iter().position(|e| e.uid == uid);
                        if moved {
                            if let Some(p) = p {
                                let mut check = b.clone();
                                if check.reposition(scene.combat.as_ref().unwrap(), &uid, p) {
                                    return Ok(Some(Command::Reposition(uid, p)));
                                }
                            }
                        }
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
        if let Some(v) = self.source_view(scene, state).filter(|v| v.modal) {
            if self.notice.is_none()
                && !self.energy_refill
                && warzone_app_client::story::current(scene, state).is_some()
            {
                if !self.story_input_ready(scene, state) {
                    return Ok(None);
                }
                let button_message = warzone_app_client::story::current(scene, state)
                    .is_some_and(|p| p.0 == "#pvp_end_game_short_info");
                match event {
                    Event::KeyDown {
                        keycode: Some(Keycode::Return | Keycode::Space),
                        repeat: false,
                        ..
                    } => return Ok(Some(Self::dialogue_command(state))),
                    Event::MouseButtonDown {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        self.source_pressed = if button_message {
                            v.hit(x, y)
                        } else {
                            Some(SourceAction::Confirm)
                        }
                    }
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        return Ok(self
                            .source_pressed
                            .take()
                            .filter(|a| !button_message || Some(*a) == v.hit(x, y))
                            .map(|_| Self::dialogue_command(state)));
                    }
                    _ => {}
                }
                return Ok(None);
            }
            if state
                .mission
                .as_ref()
                .is_some_and(|s| s.battle.result.is_some())
            {
                if let Event::KeyDown {
                    keycode: Some(Keycode::Return | Keycode::Escape),
                    repeat: false,
                    ..
                } = event
                {
                    return Ok(Some(Command::Return));
                }
                let rows = warzone_app_client::interface_result::rows(state);
                let scale = warzone_app_client::hud::scale(&self.camera);
                let size = scene
                    .interface
                    .as_ref()
                    .and_then(|c| c.symbols.get("battle_result"))
                    .map_or([364., 314.], |r| r.size);
                let left = (self.camera.width as f64 / scale - size[0]) / 2. + 92.;
                let top = (self.camera.height as f64 / scale - size[1]) / 2. + 2. + 125.;
                let row_at = |x: i32, y: i32| {
                    let row = ((y as f64 / scale - top) / 49.).floor() as i32;
                    ((0..3).contains(&row)
                        && x as f64 / scale >= left
                        && x as f64 / scale < left + 238.)
                        .then_some(row as usize)
                };
                match &event {
                    Event::MouseButtonDown {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        if let Some(row) = row_at(*x, *y) {
                            self.result_drag = Some((row, *x, self.result_scroll[row]));
                            self.result_sample = Some((*x, std::time::Instant::now()));
                            self.result_velocity[row] = 0.;
                            self.source_pressed = None;
                            return Ok(None);
                        }
                    }
                    Event::MouseMotion { x, .. } => {
                        if let Some((row, start, scroll)) = self.result_drag {
                            let max = (rows[row].len() as f64 * 44. + 30. - 238.).max(0.) * scale;
                            self.result_scroll[row] = warzone_app_client::scroll_motion::rubber(
                                scroll + (start - *x) as f64,
                                max,
                                238. * scale,
                            );
                            if let Some((old, at)) = self.result_sample {
                                self.result_velocity[row] = ((old - *x) as f64
                                    / at.elapsed().as_secs_f64().max(0.001))
                                .clamp(-2000. * scale, 2000. * scale);
                            }
                            self.result_sample = Some((*x, std::time::Instant::now()));
                            return Ok(None);
                        }
                    }
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        ..
                    } if self.result_drag.take().is_some() => {
                        self.result_sample = None;
                        return Ok(None);
                    }
                    _ => {}
                }
                if let Event::MouseWheel { y, mouse_y, .. } = event {
                    let scale = warzone_app_client::hud::scale(&self.camera);
                    let row = ((mouse_y as f64 / scale - top) / 49.).floor() as i32;
                    if (0..3).contains(&row) {
                        self.result_scroll[row as usize] =
                            (self.result_scroll[row as usize] - y as f64 * 60.).clamp(
                                0.,
                                (rows[row as usize].len() as f64 * 44. + 30. - 238.).max(0.)
                                    * scale,
                            );
                    }
                    return Ok(None);
                }
            }
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Escape | Keycode::Return),
                    repeat: false,
                    ..
                } => {
                    if matches!(
                        event,
                        Event::KeyDown {
                            keycode: Some(Keycode::Return),
                            ..
                        }
                    ) && (self.pause_confirm || self.surrender)
                    {
                        return Ok(self.source_action(SourceAction::Confirm, scene, state));
                    }
                    self.notice = None;
                    self.energy_refill = false;
                    self.objectives = false;
                    self.pause_confirm = false;
                    self.surrender = false;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    repeat: false,
                    ..
                } if self.objectives && self.notice.is_none() => {
                    self.objectives = false;
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    self.source_pressed = v.hit(x, y);
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    let down = self.source_pressed.take();
                    if let Some(action) = down.filter(|_| down == v.hit(x, y)) {
                        return Ok(self.source_action(action, scene, state));
                    }
                }
                Event::Window { .. } => {
                    self.input.event(event, &mut self.camera, &self.view);
                }
                _ => {}
            }
            return Ok(None);
        }
        if let Some(t) = b.tutorial.as_ref() {
            use warzone_app_client::battle::TutorialCommand as T;
            if let Event::KeyDown {
                keycode: Some(Keycode::Return),
                repeat: false,
                ..
            } = event
            {
                return Ok(if b.tutorial_id(scene.combat.as_ref().unwrap()) == 9 {
                    Some(Command::Tutorial(T::Leave))
                } else if b.phase == "placement" {
                    Some(Command::Start)
                } else {
                    None
                });
            }
            if matches!(event, Event::KeyDown { .. } | Event::MouseWheel { .. }) {
                return Ok(None);
            }
            if b.phase == "battle" {
                match event {
                    Event::MouseButtonDown {
                        mouse_btn: MouseButton::Left,
                        ..
                    } => self.source_pressed = Some(SourceAction::Confirm),
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        if self.source_pressed.take().is_none() {
                            return Ok(None);
                        }
                        if let Some(SourceAction::FallingPickup(sequence, id, instance)) =
                            self.source_view(scene, state).and_then(|v| v.hit(x, y))
                        {
                            self.rewards.pickup(
                                sequence,
                                id,
                                instance,
                                self.reward_clock.elapsed().as_secs_f64(),
                            );
                            return Ok(None);
                        }
                        let c = scene.combat.as_ref().unwrap();
                        if b.tutorial_id(c) == 9 {
                            return Ok(self
                                .source_view(scene, state)
                                .and_then(|v| v.hit(x, y))
                                .filter(|a| *a == SourceAction::Close)
                                .map(|_| Command::Tutorial(T::Leave)));
                        }
                        let picked = self
                            .pick_current([x as f64, y as f64], |e| {
                                b.index(&e.uid)
                                    .is_some_and(|i| b.units[i].hp > 0 && !b.units[i].damaged)
                            })
                            .and_then(|i| b.index(&self.view.entities[i].uid))
                            .filter(|i| b.units[*i].hp > 0 && !b.units[*i].damaged);
                        if let Some(i) = picked {
                            let u = &b.units[i];
                            if u.own {
                                return Ok(Some(Command::Tutorial(T::Select(u.uid.clone()))));
                            }
                            if u.enemy {
                                return Ok(Some(if t.enemy.as_ref() == Some(&u.uid) {
                                    Command::Attack(
                                        t.selected.clone().ok_or("Сначала выбери отряд")?,
                                        u.uid.clone(),
                                    )
                                } else {
                                    Command::Tutorial(T::Enemy(u.uid.clone()))
                                }));
                            }
                        }
                        let w = self.camera.world([x as f64, y as f64]);
                        let p = self.view.projection.inverse(w[0], w[1]);
                        let p = [(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32];
                        return Ok(Some(if t.route == Some(p) {
                            Command::Move(t.selected.clone().ok_or("Сначала выбери отряд")?, p)
                        } else {
                            Command::Tutorial(T::Route(p))
                        }));
                    }
                    Event::Window { .. } => {
                        self.input.event(event, &mut self.camera, &self.view);
                    }
                    _ => {}
                }
                return Ok(None);
            }
        }
        if let Event::KeyDown {
            keycode: Some(k),
            repeat: false,
            ..
        } = event
        {
            if b.result.is_some() {
                return Ok([Keycode::Return, Keycode::Escape]
                    .contains(&k)
                    .then_some(Command::Return));
            }
            if self.surrender {
                if k == Keycode::Escape {
                    self.surrender = false;
                    return Ok(None);
                }
                if k == Keycode::Return {
                    self.surrender = false;
                    return Ok(Some(Command::Surrender));
                }
                return Ok(None);
            }
            if k == Keycode::Q {
                self.card_gesture = None;
                self.list_drag = None;
                self.input.drag = None;
                self.objectives = !self.objectives;
                return Ok(None);
            }
            if self.objectives {
                if k == Keycode::Escape {
                    self.objectives = false;
                }
                return Ok(None);
            }
            match k {
                Keycode::Return => {
                    return Ok(if warzone_app_client::interface_battle::available(scene) {
                        self.source_action(SourceAction::Primary, scene, state)
                    } else {
                        Some(Self::primary(state))
                    });
                }
                Keycode::Space => return Ok((b.phase == "battle").then_some(Command::Skip)),
                Keycode::F10 => {
                    if b.phase == "battle" {
                        self.surrender = true;
                    }
                    return Ok(None);
                }
                Keycode::Tab => {
                    self.cycle(state);
                    return Ok(None);
                }
                Keycode::Delete => {
                    return Ok(if b.phase == "placement" {
                        self.input
                            .selected
                            .and_then(|i| self.view.entities.get(i))
                            .map(|e| Command::Unplace(e.uid.clone()))
                    } else {
                        None
                    });
                }
                Keycode::Escape => {
                    if self.card_gesture.take().is_some() || self.list_drag.take().is_some() {
                        return Ok(None);
                    }
                    self.input.selected = None;
                    self.selection = Default::default();
                    return Ok((b.phase == "placement").then_some(Command::CancelPlacement));
                }
                _ => {}
            }
        }
        if self.modal(state) {
            let l = modal_layout(&self.camera);
            match event {
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    self.pressed = if l.close.contains_point(Point::new(x, y)) {
                        Some(10)
                    } else if l.claim.contains_point(Point::new(x, y)) {
                        Some(11)
                    } else {
                        None
                    };
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    let pressed = self.pressed.take();
                    let hit = if l.close.contains_point(Point::new(x, y)) {
                        Some(10)
                    } else if l.claim.contains_point(Point::new(x, y)) {
                        Some(11)
                    } else {
                        None
                    };
                    if pressed.is_some() && pressed == hit {
                        if b.result.is_some() {
                            return Ok(Some(Command::Return));
                        }
                        if self.surrender && hit == Some(11) {
                            self.surrender = false;
                            return Ok(Some(Command::Surrender));
                        }
                        self.surrender = false;
                        self.objectives = false;
                    }
                }
                Event::Window { .. } => {
                    self.input.event(event, &mut self.camera, &self.view);
                }
                _ => {}
            }
            return Ok(None);
        }
        if b.phase == "placement" {
            if let Some(v) = self.source_view(scene, state) {
                match &event {
                    Event::MouseWheel { y, mouse_y, .. }
                        if *mouse_y as f64
                            >= warzone_app_client::interface_battle::bottom_top(
                                scene,
                                &self.camera,
                            ) =>
                    {
                        self.list_scroll =
                            (self.list_scroll - *y as f64 * 60.).clamp(0., v.scroll_max);
                        return Ok(None);
                    }
                    Event::MouseButtonDown {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        self.source_pressed = v.hit(*x, *y);
                        if let Some(SourceAction::Choose(index)) = self.source_pressed {
                            self.card_return = None;
                            self.unit_index = index;
                            self.card_gesture = Some(CardGesture {
                                index,
                                start: [*x, *y],
                                point: [*x, *y],
                                scroll: self.list_scroll,
                                mode: 0,
                            });
                        } else if v.blocks(*x, *y) && self.source_pressed.is_none() {
                            self.list_drag = Some((*x, self.list_scroll));
                        }
                        if v.blocks(*x, *y)
                            || self.source_pressed.is_some()
                            || warzone_app_client::hud::blocks_point(scene, &self.camera, *x, *y)
                        {
                            self.input.drag = None;
                            return Ok(None);
                        }
                        if b.tutorial.is_none() {
                            if let Some(i) = self.pick_current([*x as f64, *y as f64], |e| {
                                b.units
                                    .iter()
                                    .any(|u| u.uid == e.uid && u.hp > 0 && !u.damaged)
                            }) {
                                let e = &self.view.entities[i];
                                if let Some(u) = b.units.iter().find(|u| u.uid == e.uid) {
                                    self.input.selected = Some(i);
                                    self.selection.clear_pending();
                                    if u.own {
                                        self.selection.unit = Some(u.uid.clone());
                                        self.placement_drag =
                                            Some((u.uid.clone(), [*x, *y], [*x, *y]));
                                    } else {
                                        self.selection.unit = None;
                                        self.selection.target = Some(u.uid.clone());
                                    }
                                    self.input.drag = None;
                                    return Ok(None);
                                }
                            }
                        }
                    }
                    Event::MouseMotion { x, y, .. } => {
                        if let Some(g) = &mut self.card_gesture {
                            let old_mode = g.mode;
                            g.update(
                                [*x, *y],
                                warzone_app_client::interface_battle::bottom_top(
                                    scene,
                                    &self.camera,
                                ),
                            );
                            let dx = *x - g.start[0];
                            if g.mode == 1 {
                                self.list_scroll = (g.scroll - dx as f64).clamp(0., v.scroll_max);
                            } else if old_mode == 1 && g.mode == 2 {
                                self.list_scroll = g.scroll;
                            }
                            return Ok(None);
                        }
                        if let Some((x0, scroll)) = self.list_drag {
                            self.list_scroll = (scroll + (x0 - *x) as f64).clamp(0., v.scroll_max);
                            return Ok(None);
                        }
                    }
                    Event::MouseButtonUp {
                        mouse_btn: MouseButton::Left,
                        x,
                        y,
                        ..
                    } => {
                        self.list_drag = None;
                        if let Some(mut g) = self.card_gesture.take() {
                            // A fast release can arrive without a final motion
                            // event. Its coordinates are still the drop point.
                            let old_mode = g.mode;
                            g.update(
                                [*x, *y],
                                warzone_app_client::interface_battle::bottom_top(
                                    scene,
                                    &self.camera,
                                ),
                            );
                            if old_mode == 1 && g.mode == 2 {
                                self.list_scroll = g.scroll;
                            }
                            self.source_pressed = None;
                            self.input.drag = None;
                            // CHUDBattle.onButtonSoldierDragFinish owns the
                            // captured gesture even over another HUD button.
                            // putSoldier validates the offset cell, not the
                            // finger's overlap with the goals/resource icons.
                            if g.mode == 2
                                && *x >= 0
                                && *y >= 0
                                && *x < self.camera.width as i32
                                && *y < self.camera.height as i32
                            {
                                let id = Self::types(scene, state)[g.index];
                                let cell = warzone_app_client::interface_battle::drop_cell(
                                    scene,
                                    &self.camera,
                                    [*x, *y],
                                );
                                if b.placement_cell(scene.combat.as_ref().unwrap(), cell, id)
                                    .is_none()
                                {
                                    let to = v
                                        .hits
                                        .iter()
                                        .find(|h| h.action == SourceAction::Choose(g.index))
                                        .map_or(g.start, |h| {
                                            [
                                                (h.rect[0] + h.rect[2] * 0.5).round() as i32,
                                                (h.rect[1] + h.rect[3] * 0.5).round() as i32,
                                            ]
                                        });
                                    self.card_return =
                                        Some((id, [*x, *y], to, std::time::Instant::now()));
                                    return Ok(None);
                                }
                                return Ok(Some(Command::Place(cell, id)));
                            }
                            return Ok(None);
                        }
                        if let Some(down) = self.source_pressed.take() {
                            self.input.drag = None;
                            return Ok(if Some(down) == v.hit(*x, *y) {
                                self.source_action(down, scene, state)
                            } else {
                                None
                            });
                        }
                        if v.blocks(*x, *y)
                            || warzone_app_client::hud::blocks_point(scene, &self.camera, *x, *y)
                        {
                            self.input.drag = None;
                            return Ok(None);
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(v) = self.source_view(scene, state) {
            match &event {
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if v.hit(*x, *y).is_some() => {
                    self.source_pressed = v.hit(*x, *y);
                    self.input.drag = None;
                    return Ok(None);
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if self.source_pressed.is_some() => {
                    let down = self.source_pressed.take().unwrap();
                    self.input.drag = None;
                    return Ok(if Some(down) == v.hit(*x, *y) {
                        self.source_action(down, scene, state)
                    } else {
                        None
                    });
                }
                Event::MouseButtonDown { x, y, .. } | Event::MouseButtonUp { x, y, .. }
                    if v.blocks(*x, *y) =>
                {
                    self.input.drag = None;
                    return Ok(None);
                }
                _ => {}
            }
        }
        if let Event::MouseButtonDown { x, y, .. } | Event::MouseButtonUp { x, y, .. } = &event {
            if self
                .source_view(scene, state)
                .is_some_and(|v| v.blocks(*x, *y))
            {
                self.input.drag = None;
                return Ok(None);
            }
        }
        match &event {
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let source_placement = warzone_app_client::interface_battle::available(scene);
                self.pressed = if source_placement {
                    None
                } else {
                    self.hit(scene, *x, *y)
                };
                if self.pressed.is_some()
                    || *y < warzone_app_client::hud::height(scene, &self.camera)
                    || !warzone_app_client::interface_battle::available(scene)
                        && *y >= self.camera.height as i32 - 138
                {
                    self.input.drag = None;
                    return Ok(None);
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(button) = self.pressed.take() {
                    self.input.drag = None;
                    if self.hit(scene, *x, *y) != Some(button) {
                        return Ok(None);
                    }
                    return Ok(match button {
                        0 => Some(Self::primary(state)),
                        1 => {
                            self.cycle(state);
                            None
                        }
                        2 => {
                            self.objectives = true;
                            None
                        }
                        _ => {
                            if b.phase == "placement" {
                                Some(Command::CancelPlacement)
                            } else {
                                self.surrender = true;
                                None
                            }
                        }
                    });
                }
                let drag = self.input.drag.take();
                if drag.is_none_or(|(_, _, m)| m)
                    || *y < warzone_app_client::hud::height(scene, &self.camera)
                    || !warzone_app_client::interface_battle::available(scene)
                        && *y >= self.camera.height as i32 - 138
                {
                    return Ok(None);
                }
                let picked = self.pick_current([*x as f64, *y as f64], |e| {
                    b.units
                        .iter()
                        .find(|u| u.uid == e.uid)
                        .is_some_and(|u| u.hp > 0 && !u.damaged)
                });
                if b.phase == "battle"
                    && scene.combat.as_ref().is_some_and(|c| c.selection.is_some())
                {
                    let world = self.camera.world([*x as f64, *y as f64]);
                    let p = self.view.projection.inverse(world[0], world[1]);
                    let intent = self.selection.click(
                        b,
                        scene.combat.as_ref().unwrap(),
                        picked.map(|i| self.view.entities[i].uid.as_str()),
                        [(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32],
                    );
                    self.input.selected = self
                        .selection
                        .unit
                        .as_ref()
                        .and_then(|uid| self.view.entities.iter().position(|e| &e.uid == uid));
                    use warzone_app_client::battle_selection::Intent;
                    return Ok(match intent {
                        Intent::None => None,
                        Intent::Move(uid, p) => Some(Command::Move(uid, p)),
                        Intent::Attack(uid, t) => Some(Command::Attack(uid, t)),
                        Intent::AttackAll(t) => Some(Command::AttackAll(t)),
                        Intent::AttackFrom(uid, t, p) => Some(Command::AttackFrom(uid, t, p)),
                    });
                }
                if let Some(i) = picked {
                    let e = &self.view.entities[i];
                    if b.units.iter().any(|u| u.uid == e.uid && u.own && u.hp > 0) {
                        self.input.selected = Some(i);
                        return Ok(None);
                    }
                }
                let world = self.camera.world([*x as f64, *y as f64]);
                let p = self.view.projection.inverse(world[0], world[1]);
                let cell = [(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32];
                if b.phase == "placement" {
                    self.input.selected = None;
                    self.selection = Default::default();
                    if !warzone_app_client::interface_battle::available(scene) {
                        return Ok(Some(Command::Place(cell, self.unit_type(scene, state))));
                    }
                    return Ok(None); // Placement comes only from a captured troop-card drag.
                }
                let Some(selected) = self
                    .input
                    .selected
                    .and_then(|i| self.view.entities.get(i))
                    .map(|e| e.uid.clone())
                else {
                    return Ok(None);
                };
                return Ok(Some(if let Some(target) = picked {
                    Command::Attack(selected, self.view.entities[target].uid.clone())
                } else {
                    Command::Move(selected, cell)
                }));
            }
            _ => {}
        }
        self.input.event(event, &mut self.camera, &self.view);
        Ok(None)
    }
    fn panel(&self, scene: &Scene, state: &State) -> Option<PanelView> {
        if warzone_app_client::interface_battle::available(scene) {
            return None;
        }
        let s = state.mission.as_ref().unwrap();
        let b = &s.battle;
        let c = scene.combat.as_ref().unwrap();
        let m = &c.missions[&b.mission];
        let (title, rows, primary, ready) = if let Some(result) = &b.result {
            let lost = b
                .units
                .iter()
                .filter(|u| u.profile.is_some() && u.hp <= 0)
                .count();
            let live = b
                .units
                .iter()
                .filter(|u| u.profile.is_some() && u.hp > 0)
                .count();
            let reward = state
                .actions
                .iter()
                .rev()
                .find(|a| a.uid == s.id && (a.kind == "mission_win" || a.kind == "mission_defeat"))
                .map(|a| {
                    a.gains
                        .iter()
                        .map(|(k, n)| {
                            format!("+{n} {}", warzone_app_client::production::resource_name(k))
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                })
                .unwrap_or_default();
            (
                if result == "win" {
                    "Победа"
                } else {
                    "Поражение"
                }
                .into(),
                vec![
                    m.title.clone(),
                    format!("Вернулось отрядов: {live} · Потеряно: {lost}"),
                    if reward.is_empty() {
                        "Наград нет. Повторы пройденной миссии не выдают первую награду снова."
                            .into()
                    } else {
                        reward
                    },
                ],
                "На базу · Enter".into(),
                true,
            )
        } else if self.surrender {
            (
                "Сдаться?".into(),
                vec![
                    "Все высаженные отряды будут потеряны. Это действие сохранится сразу.".into(),
                    "Esc — продолжить бой".into(),
                ],
                "Сдаться · Enter".into(),
                true,
            )
        } else if self.objectives {
            let rows = b
                .triggers
                .iter()
                .filter_map(|t| {
                    t.actions
                        .iter()
                        .find(|a| a["name"] == "Complete_Objective")
                        .map(|a| {
                            let key = a["data"]["lang"].as_str().unwrap_or(&t.name);
                            let text = scene
                                .progression
                                .as_ref()
                                .and_then(|p| p.language.get(key))
                                .map(String::as_str)
                                .unwrap_or(key);
                            format!(
                                "{} {text}",
                                if t.done.iter().all(|x| *x) {
                                    "✓"
                                } else {
                                    "•"
                                }
                            )
                        })
                })
                .collect();
            (m.title.clone(), rows, String::new(), false)
        } else {
            return None;
        };
        Some(PanelView {
            section: "Миссия".into(),
            title,
            rows,
            awards: String::new(),
            primary_label: primary,
            index: 1,
            total: 1,
            ready,
        })
    }
    pub fn draw(
        &mut self,
        art: &mut Art,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        state: &State,
        wall: i64,
        seconds: f64,
        message: &str,
    ) -> Result<usize, String> {
        self.sync(scene, state)?;
        if self
            .playback
            .as_ref()
            .is_some_and(|(p, t)| t.elapsed().as_secs_f64() >= p.duration)
        {
            self.playback = None;
        }
        let motion = self
            .playback
            .as_ref()
            .map(|(p, t)| (p, t.elapsed().as_secs_f64()));
        let s = state.mission.as_ref().unwrap();
        let b = &s.battle;
        let c = scene.combat.as_ref().unwrap();
        let m = &c.missions[&b.mission];
        let id = self.unit_type(scene, state);
        let detail = if b.phase == "placement" {
            format!(
                "{}: {} · Доступно {} · Tab: другой тип · Delete: убрать выбранный",
                m.title,
                scene.item_name(id, &c.defs[&id].alias),
                b.available(id)
            )
        } else {
            self.input
                .selected
                .and_then(|i| self.view.entities.get(i))
                .and_then(|e| b.units.iter().find(|u| u.uid == e.uid))
                .map_or_else(
                    || {
                        "Выбери свой отряд; ЛКМ по цели — атака, по свободной клетке — движение"
                            .into()
                    },
                    |u| {
                        format!(
                            "Здоровье {}/{} · Атака {} · Дальность {} · Ход {} клеток",
                            u.hp,
                            u.hp_max,
                            u.attack,
                            u.range(),
                            u.speed
                        )
                    },
                )
        };
        let action = if b.result.is_some() {
            "На базу"
        } else if b.phase == "placement" {
            "Начать бой"
        } else {
            "Пропустить ход"
        };
        let last_log = b.log.last().map(|k| {
            scene
                .progression
                .as_ref()
                .and_then(|p| p.language.get(k))
                .map(String::as_str)
                .unwrap_or(k)
        });
        let zones = if b.phase == "placement" && b.tutorial.is_none() {
            m.zones
                .iter()
                .filter(|z| z.kind == 2)
                .map(|z| [z.x as f64, z.y as f64, z.w as f64, z.h as f64])
                .collect()
        } else {
            vec![]
        };
        let source = self.source_view(scene, state);
        let retained = source.is_some();
        art.draw_with_assets(
            canvas,
            &self.view,
            scene,
            &self.camera,
            seconds,
            self.input.selected,
            Hud {
                original: if retained { source.as_ref() } else { None },
                world_markers: if retained { None } else { source.as_ref() },
                motion,
                tactical: true,
                navigation: Some([
                    format!("{action} · Enter"),
                    "Отряды · Tab".into(),
                    "Цели · Q".into(),
                    if b.phase == "placement" {
                        "Отмена · Esc"
                    } else {
                        "Сдаться · F10"
                    }
                    .into(),
                ]),
                zones,
                preview: None,
                editing: false,
                resources: &state.resources,
                energy: state.energy(scene, wall).to_string(),
                values: self.rewards.hud_values(scene, state, wall),
                quests: if motion.is_some() || source.as_ref().is_some_and(|v| v.modal) {
                    None
                } else {
                    self.panel(scene, state)
                },
                detail: &detail,
                action,
                message: if message.is_empty() {
                    last_log.unwrap_or("ЛКМ — отряд или клетка · Перетаскивание — камера")
                } else {
                    message
                },
            },
        )
    }
    pub fn cancel_pointer(&mut self, scene: &Scene, state: &State) -> Result<(), String> {
        self.input.drag = None;
        self.source_pressed = None;
        self.card_gesture = None;
        self.list_drag = None;
        self.placement_drag = None;
        self.view = state.mission_scene(scene)?;
        Ok(())
    }
    pub fn save_camera(&self, state: &mut State) {
        if let Some(s) = &mut state.mission {
            // Closing during a cosmetic pan stores its complete destination.
            // Reopening does not replay it or repeat the trigger continuation.
            let point = self
                .camera_motion
                .as_ref()
                .map_or([self.camera.x, self.camera.y], |m| {
                    m.target(&self.view.projection, &self.camera)
                });
            s.camera = [point[0], point[1], self.camera.saved_pose()[2]];
            s.camera_serial = self.camera_serial;
        }
    }
}

/// Private deterministic SDL replay. Only --mission-script enables this entry;
/// no direct calls to the state model, so clicks exercise the ordinary UI path.
fn diagnostic_map_drag_start(
    camera: &Camera,
    hud_height: i32,
    view: Option<&SourceView>,
) -> Option<[i32; 2]> {
    [0.5, 0.3, 0.7, 0.15, 0.85]
        .into_iter()
        .flat_map(|fx| {
            [0.5, 0.35, 0.65, 0.8].into_iter().map(move |fy| {
                [
                    (camera.width as f64 * fx) as i32,
                    (camera.height as f64 * fy) as i32,
                ]
            })
        })
        .find(|p| {
            p[1] >= hud_height
                && view.is_none_or(|v| !v.blocks(p[0], p[1]) && v.hit(p[0], p[1]).is_none())
        })
}

pub fn script(
    frame: u64,
    script: &serde_json::Value,
    events: &sdl2::EventSubsystem,
    id: u32,
    controller: Option<&Ui>,
    scene: &Scene,
    base_camera: &Camera,
    state: &State,
    base_ui: &crate::ui::Ui,
) -> Result<(), String> {
    let key = |k| Event::KeyDown {
        timestamp: 0,
        window_id: id,
        keycode: Some(k),
        scancode: None,
        keymod: sdl2::keyboard::Mod::NOMOD,
        repeat: false,
    };
    let click = |p: [i32; 2]| -> Result<(), String> {
        for down in [true, false] {
            let [x, y] = p;
            events.push_event(if down {
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
            })?;
        }
        Ok(())
    };
    for step in script["steps"]
        .as_array()
        .ok_or("Missing mission script steps")?
    {
        if step["frame"].as_u64() != Some(frame) {
            continue;
        }
        let a = &step["action"];
        match a[0].as_str().ok_or("Missing script action")? {
            "pointer_down" | "pointer_move" | "pointer_up" => {
                let name = a[0].as_str().unwrap();
                let x = a[1].as_i64().ok_or("Missing pointer x")? as i32;
                let y = a[2].as_i64().ok_or("Missing pointer y")? as i32;
                if name == "pointer_move" {
                    let count = a[5].as_u64().unwrap_or(1).clamp(1, 1024);
                    let dx = a[3].as_i64().unwrap_or(0) as i32;
                    let dy = a[4].as_i64().unwrap_or(0) as i32;
                    for _ in 0..count {
                        events.push_event(Event::MouseMotion {
                            timestamp: 0,
                            window_id: id,
                            which: 0,
                            mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                            x,
                            y,
                            xrel: dx,
                            yrel: dy,
                        })?;
                    }
                } else if name == "pointer_down" {
                    events.push_event(Event::MouseButtonDown {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    })?;
                } else {
                    events.push_event(Event::MouseButtonUp {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    })?;
                }
            }
            "base_focus" => {
                if controller.is_some() {
                    return Err("Base focus inside mission".into());
                }
                let uid = a[1].as_str().ok_or("Missing base entity")?;
                let entities = state.world_entities_at(scene, None)?;
                let e = entities
                    .iter()
                    .find(|e| e.uid == uid)
                    .ok_or("Missing base entity")?;
                let p = base_camera.screen(scene.projection.project(e.x, e.y));
                let (dx, dy) = (
                    (base_camera.width / 2) as i32 - p[0].round() as i32,
                    (base_camera.height / 2) as i32 - p[1].round() as i32,
                );
                if dx == 0 && dy == 0 {
                    continue;
                }
                // A zero-distance focus gesture used to tap a bobbing ready
                // marker at the camera center. Pan from a point without any
                // source UI target instead, using ordinary SDL mouse events.
                let source = base_ui.original_view(scene, state, base_camera);
                let (x, y) = [0.5, 0.7, 0.3, 0.85]
                    .into_iter()
                    .flat_map(|fx| {
                        [0.5, 0.7, 0.3].into_iter().map(move |fy| {
                            (
                                (base_camera.width as f64 * fx) as i32,
                                (base_camera.height as f64 * fy) as i32,
                            )
                        })
                    })
                    .find(|(x, y)| {
                        source.as_ref().is_none_or(|v| {
                            !v.hits.iter().any(|h| {
                                *x as f64 >= h.rect[0]
                                    && (*x as f64) < h.rect[0] + h.rect[2]
                                    && *y as f64 >= h.rect[1]
                                    && (*y as f64) < h.rect[1] + h.rect[3]
                            })
                        })
                    })
                    .ok_or("No unblocked source point for camera pan")?;
                events.push_event(Event::MouseButtonDown {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x,
                    y,
                })?;
                events.push_event(Event::MouseMotion {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                    x: x + dx,
                    y: y + dy,
                    xrel: dx,
                    yrel: dy,
                })?;
                events.push_event(Event::MouseButtonUp {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: x + dx,
                    y: y + dy,
                })?;
            }
            "screen_drag" | "deploy" => {
                let point = |v: &serde_json::Value| -> Result<(i32, i32), String> {
                    let x = v[0].as_i64().ok_or("Missing screen x")? as i32;
                    let y = v[1].as_i64().ok_or("Missing screen y")? as i32;
                    if x < 0
                        || y < 0
                        || x >= base_camera.width as i32
                        || y >= base_camera.height as i32
                    {
                        return Err("Screen drag outside viewport".into());
                    }
                    Ok((x, y))
                };
                let ((x, y), (tx, ty)) = if a[0] == "deploy" {
                    let ui = controller.ok_or("No deployment controller")?;
                    let kind = a[1].as_u64().ok_or("Missing deployment type")? as u32;
                    let index = Ui::types(scene, state)
                        .iter()
                        .position(|id| *id == kind)
                        .ok_or("No source unit card")?;
                    let view = ui
                        .source_view(scene, state)
                        .ok_or("No source deployment view")?;
                    let h = view
                        .hits
                        .iter()
                        .find(|h| h.action == SourceAction::Choose(index))
                        .ok_or("No available source unit card")?;
                    let p = ui.camera.screen(scene.projection.project(
                        a[2][0].as_f64().ok_or("Missing drop x")?,
                        a[2][1].as_f64().ok_or("Missing drop y")?,
                    ));
                    let scale = warzone_app_client::hud::scale(&ui.camera);
                    (
                        (
                            (h.rect[0] + h.rect[2] / 2.).round() as i32,
                            (h.rect[1] + h.rect[3] / 2.).round() as i32,
                        ),
                        (
                            (p[0] - 2. * scale).round() as i32,
                            (p[1] - 25. * scale).round() as i32,
                        ),
                    )
                } else {
                    (point(&a[1])?, point(&a[2])?)
                };
                events.push_event(Event::MouseButtonDown {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x,
                    y,
                })?;
                if a[0] == "deploy" {
                    // Leave the horizontal card scroller vertically before
                    // crossing the map, as an actual deployment gesture does.
                    events.push_event(Event::MouseMotion {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                        x,
                        y: y - 40,
                        xrel: 0,
                        yrel: -40,
                    })?;
                }
                events.push_event(Event::MouseMotion {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                    x: tx,
                    y: ty,
                    xrel: tx - x,
                    yrel: ty - y,
                })?;
                events.push_event(Event::MouseButtonUp {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: tx,
                    y: ty,
                })?;
            }
            "base_place" => {
                if controller.is_some() {
                    return Err("Base placement inside mission".into());
                }
                let (from, anchor) = base_ui
                    .preview_drag_geometry
                    .ok_or("No preview for placement drag")?;
                let p = base_camera.screen(scene.projection.project(
                    a[1].as_f64().ok_or("Missing placement x")?,
                    a[2].as_f64().ok_or("Missing placement y")?,
                ));
                let to = [p[0] + from[0] - anchor[0], p[1] + from[1] - anchor[1]];
                let (from, to) = (from.map(|n| n.round() as i32), to.map(|n| n.round() as i32));
                events.push_event(Event::MouseButtonDown {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: from[0],
                    y: from[1],
                })?;
                events.push_event(Event::MouseMotion {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                    x: to[0],
                    y: to[1],
                    xrel: to[0] - from[0],
                    yrel: to[1] - from[1],
                })?;
                events.push_event(Event::MouseButtonUp {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: to[0],
                    y: to[1],
                })?;
            }
            "base_hover" => {
                if controller.is_some() {
                    return Err("Base hover inside mission".into());
                }
                let x = a
                    .get(1)
                    .and_then(serde_json::Value::as_f64)
                    .ok_or("Missing map x")?;
                let y = a
                    .get(2)
                    .and_then(serde_json::Value::as_f64)
                    .ok_or("Missing map y")?;
                let p = base_camera.screen(scene.projection.project(x, y));
                events.push_event(Event::MouseMotion {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    mousestate: sdl2::mouse::MouseState::from_sdl_state(0),
                    x: p[0].round() as i32,
                    y: p[1].round() as i32,
                    xrel: 0,
                    yrel: 0,
                })?;
            }
            "base_pick" => {
                if controller.is_some() {
                    return Err("Base pick inside mission".into());
                }
                let uid = a[1].as_str().ok_or("Missing base entity")?;
                let entities = state.world_entities_at(scene, None)?;
                let entity = entities
                    .iter()
                    .find(|e| e.uid == uid)
                    .ok_or("Missing base entity")?;
                let p = base_camera.screen(scene.projection.project(entity.x, entity.y));
                if p[0] < 0.
                    || p[1] < 0.
                    || p[0] >= base_camera.width as f64
                    || p[1] >= base_camera.height as f64
                {
                    return Err("Base entity outside diagnostic viewport".into());
                }
                click([(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32])?;
            }
            "battle_ui" => {
                let ui = controller.ok_or("Tactical UI outside mission")?;
                let a = match a[1].as_str().unwrap_or("") {
                    "energy" => SourceAction::Energy,
                    "energy_buy" => SourceAction::EnergyBuy,
                    "energy_wait" => SourceAction::EnergyWait,
                    "primary" => SourceAction::Primary,
                    "close" => SourceAction::Close,
                    "goals" => SourceAction::Quest(0),
                    "dismiss" => SourceAction::Dismiss,
                    "confirm" => SourceAction::Confirm,
                    "surrender" | "finish" => SourceAction::Cancel,
                    "choose" => {
                        SourceAction::Choose(a[2].as_u64().ok_or("Missing troop index")? as usize)
                    }
                    _ => return Err("Unknown tactical UI action".into()),
                };
                let v = ui
                    .source_view(scene, state)
                    .ok_or("Original tactical UI unavailable")?;
                let h = v
                    .hits
                    .iter()
                    .find(|h| h.action == a)
                    .ok_or("Original tactical action unavailable")?;
                click([
                    (h.rect[0] + h.rect[2] * 0.5).round() as i32,
                    (h.rect[1] + h.rect[3] * 0.5).round() as i32,
                ])?;
            }
            "deployment_press" | "deployment_move" | "deployment_release" => {
                let ui = controller.ok_or("Deployment drag outside mission")?;
                let name = a[0].as_str().unwrap();
                let (x, y) = if name == "deployment_press" {
                    let kind = a[1].as_u64().ok_or("Missing troop type")? as u32;
                    let index = Ui::types(scene, state)
                        .iter()
                        .position(|id| *id == kind)
                        .ok_or("Troop absent from roster")?;
                    let v = ui
                        .source_view(scene, state)
                        .ok_or("No source deployment UI")?;
                    let h = v
                        .hits
                        .iter()
                        .find(|h| h.action == SourceAction::Choose(index))
                        .ok_or("Troop thumbnail unavailable")?;
                    (
                        (h.rect[0] + h.rect[2] * 0.5).round() as i32,
                        (h.rect[1] + h.rect[3] * 0.5).round() as i32,
                    )
                } else {
                    let p = ui.camera.screen(scene.projection.project(
                        a[1][0].as_f64().ok_or("Missing drop x")?,
                        a[1][1].as_f64().ok_or("Missing drop y")?,
                    ));
                    let scale = warzone_app_client::hud::scale(&ui.camera);
                    (
                        (p[0] - 2. * scale).round() as i32,
                        (p[1] - 25. * scale).round() as i32,
                    )
                };
                events.push_event(match name {
                    "deployment_press" => Event::MouseButtonDown {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    },
                    "deployment_move" => Event::MouseMotion {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                        x,
                        y,
                        xrel: 0,
                        yrel: 0,
                    },
                    _ => Event::MouseButtonUp {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    },
                })?;
            }
            "ui_scroll" => {
                events.push_event(Event::MouseWheel {
                    timestamp: 0,
                    window_id: id,
                    which: 0,
                    x: 0,
                    y: a[1].as_i64().ok_or("Missing scroll delta")? as i32,
                    direction: sdl2::mouse::MouseWheelDirection::Normal,
                    precise_x: 0.,
                    precise_y: a[1].as_f64().unwrap() as f32,
                    mouse_x: (base_camera.width / 2) as i32,
                    mouse_y: (base_camera.height - 60) as i32,
                })?;
            }
            "ui" => {
                use warzone_app_client::interface::Action as A;
                let action = match a[1].as_str().unwrap_or("") {
                    "show_quest" => A::ShowQuest(
                        a[2].as_u64().ok_or("Missing quest ID")? as u32,
                        a[3].as_u64().ok_or("Missing exercise index")? as usize,
                    ),
                    "land_open" => A::LandOpen(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing land ID")? as u32,
                    ),
                    "land_coins" => A::LandCoins,
                    "land_cash" => A::LandCash,
                    "land_topup" => A::LandTopup,
                    "bonus_pickup" => A::BonusPickup(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing bonus recipe")? as u32,
                    ),
                    "falling_pickup" => {
                        let id = a
                            .get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing falling item")? as u32;
                        let receipt = state
                            .actions
                            .iter()
                            .rev()
                            .find(|a| matches!(a.kind.as_str(), "collect" | "workshop_collect"))
                            .ok_or("Missing production reward")?;
                        A::FallingPickup(
                            receipt.sequence,
                            id,
                            a.get(3).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize,
                        )
                    }
                    "inventory" => A::Inventory,
                    "material_info" => A::MaterialInfo(
                        a.get(2)
                            .and_then(|v| v.as_u64())
                            .ok_or("Expected material ID")? as u32,
                    ),
                    "buy_material" => A::BuyMaterial(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing material id")? as u32,
                    ),
                    "buy_research" => A::BuyResearch(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing research id")? as u32,
                    ),
                    "world_ready" | "world_ready_double" => A::ObjectPrimary(
                        state
                            .entities
                            .keys()
                            .position(|uid| {
                                Some(uid.as_str()) == a.get(2).and_then(serde_json::Value::as_str)
                            })
                            .ok_or("Missing ready marker entity")?,
                    ),
                    "shop_tab" => A::ShopTab(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing shop tab")? as u32,
                    ),
                    "shop_buy" => A::ShopBuy(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing shop item")? as u32,
                    ),
                    "shop_unlock" => A::ShopUnlock(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing shop item")? as u32,
                    ),
                    "upgrade_unit" | "upgrade_open" | "upgrade_info" | "upgrade_start"
                    | "upgrade_buy" => {
                        let id = a
                            .get(2)
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or("Missing upgrade action ID")?;
                        match a[1].as_str().unwrap() {
                            "upgrade_unit" => A::UpgradeUnit(id),
                            "upgrade_open" => A::UpgradeOpen(id),
                            "upgrade_info" => A::UpgradeInfo(id),
                            "upgrade_start" => A::UpgradeStart(id),
                            _ => A::UpgradeBuy(id),
                        }
                    }
                    "resource_speedup" => A::ResourceSpeedup,
                    "recipe_access" => A::RecipeAccess(
                        a.get(2)
                            .and_then(|v| v.as_u64())
                            .ok_or("Missing recipe id")? as u32,
                    ),
                    "research_access" => A::ResearchAccess(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing research id")? as u32,
                    ),
                    "claim_quest" => A::ClaimQuest(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing quest id")? as u32,
                    ),
                    "quest" => {
                        A::Quest(a.get(2).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize)
                    }
                    "achievements" => A::Achievements,
                    "leagues" => A::PvpLeagues,
                    "leagues_help" => A::LeaguesHelp,
                    "league_select" => A::LeagueSelect(
                        a.get(2)
                            .and_then(|v| v.as_u64())
                            .ok_or("Missing league index")? as usize,
                    ),
                    "claim_achievement" => A::ClaimAchievement(
                        a[2].as_u64().ok_or("Missing achievement")? as u32,
                        a[3].as_u64().ok_or("Missing achievement tier")? as usize,
                    ),
                    "bank" => A::Bank,
                    "bank_claim" => A::BankClaim(
                        a.get(2)
                            .and_then(|v| v.as_u64())
                            .ok_or("Missing bank offer")? as u32,
                    ),
                    "energy" => A::Energy,
                    "energy_buy" => A::EnergyBuy,
                    "energy_wait" => A::EnergyWait,
                    "primary" => A::Primary,
                    "produce" => A::Produce(
                        a.get(2).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize,
                    ),
                    "choose" => {
                        A::Choose(a.get(2).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize)
                    }
                    "dismiss" => A::Dismiss,
                    "forum" => A::Forum,
                    "settings" => A::Settings,
                    "close" => A::Close,
                    "tutorial_cancel" => A::TutorialCancel,
                    "music" => A::Music,
                    "language" => A::Language,
                    "english" => A::English,
                    "russian" => A::Russian,
                    "chinese" => A::Chinese,
                    "sound" => A::Sound,
                    "move" => A::Move,
                    "rotate" => A::Rotate,
                    "sell" => A::Sell,
                    "confirm" => A::Confirm,
                    "cancel" => A::Cancel,
                    "construction" => A::Construction,
                    "missions" => A::Missions,
                    "pvp" => A::Pvp,
                    "mission_select" => A::MissionSelect(
                        a.get(2)
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("Missing mission index")? as usize,
                    ),
                    _ => return Err("Unknown source UI action".into()),
                };
                let view = base_ui
                    .original_view(scene, state, base_camera)
                    .ok_or("No source interface for diagnostic click")?;
                let h = view
                    .hits
                    .iter()
                    .find(|h| h.action == action)
                    .ok_or("Source interface action unavailable")?;
                click([
                    (h.rect[0] + h.rect[2] * 0.5).round() as i32,
                    (h.rect[1] + h.rect[3] * 0.5).round() as i32,
                ])?;
                if a[1].as_str() == Some("world_ready_double") {
                    click([
                        (h.rect[0] + h.rect[2] * 0.5).round() as i32,
                        (h.rect[1] + h.rect[3] * 0.5).round() as i32,
                    ])?;
                }
            }
            "key" => {
                let k = match a[1].as_str().unwrap_or("") {
                    "F5" => Keycode::F5,
                    "F2" => Keycode::F2,
                    "F6" => Keycode::F6,
                    "F7" => Keycode::F7,
                    "H" => Keycode::H,
                    "M" => Keycode::M,
                    "R" => Keycode::R,
                    "L" => Keycode::L,
                    "U" => Keycode::U,
                    "N" => Keycode::N,
                    "Enter" => Keycode::Return,
                    "Space" => Keycode::Space,
                    "Esc" => Keycode::Escape,
                    "Tab" => Keycode::Tab,
                    "Q" => Keycode::Q,
                    "F10" => Keycode::F10,
                    "Delete" => Keycode::Delete,
                    "Right" => Keycode::Right,
                    "Left" => Keycode::Left,
                    _ => return Err("Unknown diagnostic key".into()),
                };
                events.push_event(key(k))?;
            }
            "focus" => {
                let ui = controller.ok_or("No camera for diagnostic drag")?;
                let b = &state.mission.as_ref().ok_or("No current battle")?.battle;
                let unit = &b.units[b
                    .index(a[1].as_str().ok_or("Missing focus squad")?)
                    .ok_or("Unknown focus squad")?];
                let mut center = [unit.x - 16., unit.y - 16.];
                if let Some(target) = a[2].as_str() {
                    let t = &b.units[b.index(target).ok_or("Unknown camera target")?];
                    center = [(unit.x + t.x) * 0.5 - 16., (unit.y + t.y) * 0.5 - 16.];
                } else if a[2].is_array() {
                    center = [
                        (center[0] + a[2][0].as_f64().ok_or("Invalid camera cell")?) * 0.5,
                        (center[1] + a[2][1].as_f64().ok_or("Invalid camera cell")?) * 0.5,
                    ];
                }
                let p = ui
                    .camera
                    .screen(scene.projection.project(center[0], center[1]));
                let (dx, dy) = (
                    (ui.camera.width / 2) as i32 - p[0].round() as i32,
                    (ui.camera.height / 2) as i32 - p[1].round() as i32,
                );
                if dx.abs() + dy.abs() > crate::input_events::slop(5) {
                    // A FallingItem can own the viewport center while the
                    // source shot's bonuses scatter. Start the same ordinary
                    // SDL drag on clear map instead of picking up that item.
                    let [x, y] = ui.diagnostic_drag_start(scene, state)?;
                    events.push_event(Event::MouseButtonDown {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x,
                        y,
                    })?;
                    events.push_event(Event::MouseMotion {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                        x: x + dx,
                        y: y + dy,
                        xrel: dx,
                        yrel: dy,
                    })?;
                    events.push_event(Event::MouseButtonUp {
                        timestamp: 0,
                        window_id: id,
                        which: 0,
                        mouse_btn: MouseButton::Left,
                        clicks: 1,
                        x: x + dx,
                        y: y + dy,
                    })?;
                }
            }
            "nav" => {
                let camera = controller.map_or(base_camera, |u| &u.camera);
                let i = a[1].as_u64().ok_or("Missing nav index")? as usize;
                if let Some(ui) = controller {
                    if let Some(v) = ui.source_view(scene, state) {
                        if i == 1 {
                            events.push_event(key(Keycode::Tab))?;
                            continue;
                        }
                        if state
                            .mission
                            .as_ref()
                            .is_some_and(|m| m.battle.result.is_some())
                            && i == 0
                        {
                            events.push_event(key(Keycode::Return))?;
                            continue;
                        }
                        let action = match i {
                            0 => SourceAction::Primary,
                            2 => SourceAction::Quest(0),
                            3 => SourceAction::Close,
                            _ => return Err("Unknown tactical navigation index".into()),
                        };
                        let h = v
                            .hits
                            .iter()
                            .find(|h| h.action == action)
                            .ok_or("Source tactical navigation unavailable")?;
                        click([
                            (h.rect[0] + h.rect[2] / 2.).round() as i32,
                            (h.rect[1] + h.rect[3] / 2.).round() as i32,
                        ])?;
                        continue;
                    }
                }
                let r = warzone_app_client::hud::navigation(scene, camera);
                let r = r.get(i).ok_or("Unknown navigation action")?;
                click([r[0] + r[2] / 2, r[1] + r[3] / 2])?;
            }
            "claim" => {
                if let Some(ui) = controller {
                    if let Some(v) = ui.source_view(scene, state).filter(|v| v.modal) {
                        let h = v
                            .hits
                            .iter()
                            .find(|h| h.action == SourceAction::Confirm)
                            .or_else(|| v.hits.iter().find(|h| h.action == SourceAction::Dismiss));
                        if let Some(h) = h {
                            click([
                                (h.rect[0] + h.rect[2] / 2.).round() as i32,
                                (h.rect[1] + h.rect[3] / 2.).round() as i32,
                            ])?;
                            continue;
                        }
                    }
                }
                let camera = controller.map_or(base_camera, |u| &u.camera);
                if controller.is_none()
                    && base_ui.kind == Some(crate::ui::Kind::Missions)
                    && warzone_app_client::interface_campaign::available(scene)
                {
                    // Preserve old diagnostic "claim" scripts with the actual
                    // two-step source flow: choose a marker, then Start.
                    if !base_ui.campaign_briefing() {
                        events.push_event(key(Keycode::Return))?;
                    }
                    let completed = scene
                        .combat
                        .as_ref()
                        .and_then(|c| c.campaign().nth(base_ui.index))
                        .is_some_and(|m| {
                            state
                                .systems
                                .as_ref()
                                .is_some_and(|p| p.flag(&format!("Mission-{}", m.mission_id)))
                        });
                    if !completed {
                        events.push_event(key(Keycode::Return))?;
                    }
                } else {
                    let r = modal_layout(camera).claim;
                    click([r.x() + 20, r.y() + 15])?;
                }
            }
            "place" | "move" | "attack" | "move_confirm" | "attack_confirm" => {
                let ui = controller.ok_or("Battle script action outside mission")?;
                let b = &state
                    .mission
                    .as_ref()
                    .ok_or("Missing script mission")?
                    .battle;
                let project = |x: f64, y: f64| {
                    let p = ui.camera.screen(scene.projection.project(x, y));
                    [(p[0] + 1e-9).floor() as i32, (p[1] + 1e-9).floor() as i32]
                };
                let pick_point = |uid: &str| -> Result<[i32; 2], String> {
                    ui.diagnostic_pick_point(scene,state,uid)?.ok_or_else(||"Transient bonus occludes diagnostic input; wait before admitting the step".into())
                };
                if a[0] != "place" {
                    let u = b
                        .index(a[1].as_str().ok_or("Missing selected squad")?)
                        .ok_or("Missing script squad")?;
                    let u = &b.units[u];
                    click(pick_point(&u.uid)?)?;
                }
                if a[0] == "attack" || a[0] == "attack_confirm" {
                    let u = b
                        .index(a[2].as_str().ok_or("Missing target")?)
                        .ok_or("Missing script target")?;
                    let u = &b.units[u];
                    click(pick_point(&u.uid)?)?;
                    if a[0] == "attack_confirm" {
                        click(pick_point(&u.uid)?)?;
                    }
                } else {
                    let p = if a[0] == "place" { &a[1] } else { &a[2] };
                    click(project(
                        p[0].as_f64().ok_or("Missing cell x")?,
                        p[1].as_f64().ok_or("Missing cell y")?,
                    ))?;
                    if a[0] == "move_confirm" {
                        click(project(p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))?;
                    }
                }
            }
            _ => return Err("Unknown mission script operation".into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod diagnostic_drag_checks {
    use super::*;
    #[test]
    fn camera_drag_avoids_real_falling_item_hit_rect_and_modal_windows() {
        let camera = Camera {
            x: 0.,
            y: 0.,
            zoom: 1.,
            width: 960,
            height: 640,
        };
        let mut view = SourceView::default();
        view.hits.push(warzone_app_client::interface::Hit {
            action: SourceAction::FallingPickup(12, 1, 0),
            rect: [450., 285., 60., 70.],
        });
        let point = diagnostic_map_drag_start(&camera, 124, Some(&view)).unwrap();
        assert_ne!(point, [480, 320]);
        assert!(view.hit(point[0], point[1]).is_none());
        view.blocks.push([0., 0., 960., 260.]);
        let point = diagnostic_map_drag_start(&camera, 124, Some(&view)).unwrap();
        assert!(point[1] >= 260);
        view.modal = true;
        assert!(diagnostic_map_drag_start(&camera, 124, Some(&view)).is_none());
    }

    #[test]
    fn deployment_accepts_sideways_starts_and_release_without_a_last_motion() {
        use warzone_app_client::{
            battle::{ArmyUnit, Battle},
            mission::Session,
        };
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/pack");
        let scene = Scene::load(&pack).unwrap();
        let seed =
            warzone_app_client::save::digest(&std::fs::read(pack.join("scene.json")).unwrap());
        let mut state = State::seed(&scene, seed).unwrap();
        let config = scene.combat.as_ref().unwrap();
        let battle = Battle::new(
            config,
            "mission_1",
            vec![ArmyUnit {
                uid: "drag-check".into(),
                type_id: 2001,
            }],
        )
        .unwrap();
        state.mission = Some(Session {
            camera_serial: 0,
            camera: config.views["mission_1"].camera,
            id: "drag-check".into(),
            opened: state.last_action_time,
            reward_eligible: false,
            battle,
        });
        for (width, height) in [(960, 640), (1920, 1080)] {
            for path in [
                "vertical",
                "sideways-first",
                "release-only",
                "horizontal-scroll",
            ] {
                let mut ui = Ui::new(&scene, &state, width, height).unwrap();
                let zone = config.missions["mission_1"]
                    .zones
                    .iter()
                    .find(|z| z.kind == 2)
                    .unwrap();
                let focus = scene
                    .projection
                    .project((zone.x + zone.w / 2) as f64, (zone.y + zone.h / 2) as f64);
                ui.camera.x = focus[0];
                ui.camera.y = focus[1];
                ui.camera.zoom = 1.;
                let view = ui.source_view(&scene, &state).unwrap();
                let hit = view
                    .hits
                    .iter()
                    .find(|h| h.action == SourceAction::Choose(0))
                    .unwrap();
                let start = [
                    (hit.rect[0] + hit.rect[2] / 2.) as i32,
                    (hit.rect[1] + hit.rect[3] / 2.) as i32,
                ];
                let scale = warzone_app_client::hud::scale(&ui.camera);
                let battle = &state.mission.as_ref().unwrap().battle;
                let end = (zone.x..zone.x + zone.w)
                    .flat_map(|x| (zone.y..zone.y + zone.h).map(move |y| [x, y]))
                    .filter(|&p| battle.placement_cell(config, p, 2001).is_some())
                    .map(|p| {
                        let screen = ui.camera.screen(
                            scene
                                .projection
                                .project(p[0] as f64 + 0.5, p[1] as f64 + 0.5),
                        );
                        [
                            (screen[0] - 2. * scale).round() as i32,
                            (screen[1] - 25. * scale).round() as i32,
                        ]
                    })
                    .find(|p| {
                        p[0] > 0
                            && p[0] < width as i32
                            && p[1] > 80
                            && (p[1] as f64)
                                < warzone_app_client::interface_battle::bottom_top(
                                    &scene, &ui.camera,
                                ) - 10.
                    })
                    .unwrap();
                let edge = |down, p: [i32; 2]| {
                    if down {
                        Event::MouseButtonDown {
                            timestamp: 0,
                            window_id: 0,
                            which: 0,
                            mouse_btn: MouseButton::Left,
                            clicks: 1,
                            x: p[0],
                            y: p[1],
                        }
                    } else {
                        Event::MouseButtonUp {
                            timestamp: 0,
                            window_id: 0,
                            which: 0,
                            mouse_btn: MouseButton::Left,
                            clicks: 1,
                            x: p[0],
                            y: p[1],
                        }
                    }
                };
                let motion = |p: [i32; 2]| Event::MouseMotion {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    mousestate: sdl2::mouse::MouseState::from_sdl_state(1),
                    x: p[0],
                    y: p[1],
                    xrel: 0,
                    yrel: 0,
                };
                assert!(
                    ui.event(edge(true, start), &scene, &state)
                        .unwrap()
                        .is_none()
                );
                if matches!(path, "sideways-first" | "horizontal-scroll") {
                    ui.event(motion([start[0] + 20, start[1]]), &scene, &state)
                        .unwrap();
                }
                if matches!(path, "vertical" | "sideways-first") {
                    ui.event(motion(end), &scene, &state).unwrap();
                }
                let release = if path == "horizontal-scroll" {
                    [start[0] + 40, start[1]]
                } else {
                    end
                };
                let command = ui.event(edge(false, release), &scene, &state).unwrap();
                if path == "horizontal-scroll" {
                    assert!(command.is_none());
                } else {
                    assert!(
                        matches!(command, Some(Command::Place(_, 2001))),
                        "{path} at {width}x{height}: {command:?}"
                    );
                }
            }
        }
    }
}
