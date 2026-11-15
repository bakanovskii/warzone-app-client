//! Temporary native layout controls; the candidate never mutates the live save.
use sdl2::{event::Event, keyboard::Keycode, mouse::MouseButton};
use warzone_app_client::{
    Camera, Entity, Scene,
    production::{ProductionSource, State},
    world::{self, LayoutEdit},
};

#[derive(Default)]
pub struct Editor {
    pub edit: Option<LayoutEdit>,
    pub purchase: Option<u32>,
    pub pay_shortfall: bool,
    pub training: Option<(ProductionSource, u32)>,
    pressed: bool,
    object_drag: Option<([i32; 2], [i32; 2])>,
    map_drag: bool,
    button: Option<bool>,
}
pub enum ResultEvent {
    Pass,
    Consumed,
    Commit(LayoutEdit),
}
impl Editor {
    pub fn owns_pointer(&self) -> bool {
        self.object_drag.is_some() || self.map_drag
    }
    pub fn cancel_pointer(&mut self) {
        self.object_drag = None;
        self.map_drag = false;
        self.pressed = false;
        self.button = None;
    }
    pub fn cancel(&mut self) {
        self.edit = None;
        self.purchase = None;
        self.pay_shortfall = false;
        self.training = None;
        self.pressed = false;
        self.object_drag = None;
        self.map_drag = false;
        self.button = None;
    }
    pub fn plan_building(
        &mut self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
        id: u32,
    ) -> Result<(), String> {
        if scene
            .construction
            .as_ref()
            .is_some_and(|c| c.buildings.contains_key(&id))
        {
            state.building_placement_offer(scene, id)?;
        } else {
            state.shop_placement_offer(scene, id)?;
        }
        let p = if id == 1027 {
            // SceneMain.findFreePlaceFromPtX:andY:toItem: fixes the first and
            // second harvester fields, rather than searching arbitrary land.
            if state.building_count(1027) == 0 {
                [39., 51.]
            } else {
                [75., 50.]
            }
        } else {
            scene.projection.inverse(camera.x, camera.y)
        };
        self.cancel();
        self.purchase = Some(id);
        self.edit = Some(LayoutEdit {
            uid: String::new(),
            x: p[0].round().clamp(-16.0, 111.0) as i32,
            y: p[1].round().clamp(-16.0, 111.0) as i32,
            direction: world::SW,
            rotation_only: false,
        });
        Ok(())
    }
    pub fn plan_training(
        &mut self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
        source: ProductionSource,
        id: u32,
        wall: i64,
    ) -> Result<(), String> {
        state.unit_ready(scene, &source.uid, id, source.started, wall)?;
        let p = scene.projection.inverse(camera.x, camera.y);
        self.cancel();
        self.training = Some((source, id));
        self.edit = Some(LayoutEdit {
            uid: String::new(),
            x: p[0].round().clamp(-16.0, 111.0) as i32,
            y: p[1].round().clamp(-16.0, 111.0) as i32,
            direction: world::SW,
            rotation_only: false,
        });
        Ok(())
    }
    pub fn event(
        &mut self,
        event: &Event,
        camera: &Camera,
        seed: &Scene,
        view: &Scene,
        state: &State,
        selected: Option<usize>,
        message: &mut String,
    ) -> ResultEvent {
        let key = match event {
            Event::KeyDown {
                keycode: Some(k),
                repeat: false,
                ..
            } => Some(*k),
            _ => None,
        };
        if self.edit.is_none() {
            // Screen buttons share the same action as M/R. Bottom panel clicks
            // cannot leak into camera picking or production controls.
            let requested = match event {
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if seed.interface.is_none()
                    && (camera.height as i32 - 132..camera.height as i32 - 106).contains(y)
                    && (camera.width as i32 - 320..camera.width as i32 - 14).contains(x) =>
                {
                    self.button = Some(*x >= camera.width as i32 - 164);
                    return ResultEvent::Consumed;
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if seed.interface.is_none()
                    && (camera.height as i32 - 132..camera.height as i32 - 106).contains(y)
                    && (camera.width as i32 - 320..camera.width as i32 - 14).contains(x) =>
                {
                    let rotate = *x >= camera.width as i32 - 164;
                    if self.button.take() != Some(rotate) {
                        return ResultEvent::Consumed;
                    }
                    Some(rotate)
                }
                _ => match key {
                    Some(Keycode::M) => Some(false),
                    Some(Keycode::R) => Some(true),
                    _ => None,
                },
            };
            if let Some(rotate) = requested {
                self.button = None;
                if let Some(e) = selected.and_then(|i| view.entities.get(i)) {
                    match state.begin_layout(seed, &e.uid, rotate) {
                        Ok(edit) => {
                            self.edit = Some(edit);
                            *message =
                                "Выбери место · R: поворот · Enter: сохранить · Esc: отмена".into();
                        }
                        Err(text) => *message = text,
                    }
                }
                return ResultEvent::Consumed;
            }
            // Any other release invalidates a partially pressed editor button.
            if matches!(event, Event::MouseButtonUp { .. }) {
                self.pressed = false;
                self.button = None;
            }
            return ResultEvent::Pass;
        }
        // SceneMain.onTouch0xa4b89 distinguishes the editing object from
        // background scrolling. Hover never moves an object; drag preserves
        // the grab offset instead of snapping its anchor to the pointer.
        if seed.interface.is_some() {
            match event {
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    let hit = self.preview(seed, view, state).is_some_and(|(e, _, _)| {
                        let p = view.projection.project(e.x, e.y);
                        e.tracks
                            .iter()
                            .flat_map(|t| t.frames.first().into_iter().flatten())
                            .any(|part| {
                                let q = camera.screen([p[0] + part.rect[0], p[1] + part.rect[1]]);
                                *x as f64 >= q[0]
                                    && *x as f64 <= q[0] + part.rect[2] * camera.zoom
                                    && *y as f64 >= q[1]
                                    && *y as f64 <= q[1] + part.rect[3] * camera.zoom
                            })
                    });
                    let e = self.edit.as_ref().unwrap();
                    if hit && !e.rotation_only {
                        self.object_drag = Some(([*x, *y], [e.x, e.y]));
                        return ResultEvent::Consumed;
                    }
                    self.map_drag = true;
                    return ResultEvent::Pass;
                }
                Event::MouseMotion { x, y, .. } => {
                    if self.map_drag {
                        return ResultEvent::Pass;
                    }
                    if let Some((grab, start)) = self.object_drag {
                        let a = camera.world(grab.map(|v| v as f64));
                        let b = camera.world([*x as f64, *y as f64]);
                        let a = view.projection.inverse(a[0], a[1]);
                        let b = view.projection.inverse(b[0], b[1]);
                        let e = self.edit.as_mut().unwrap();
                        e.x = (start[0] as f64 + b[0] - a[0]).round().clamp(-16., 111.) as i32;
                        e.y = (start[1] as f64 + b[1] - a[1]).round().clamp(-16., 111.) as i32;
                    }
                    return ResultEvent::Consumed;
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    self.map_drag = false;
                    self.object_drag = None;
                    return ResultEvent::Consumed;
                }
                Event::MouseWheel { .. } => return ResultEvent::Pass,
                _ => {}
            }
        }
        let edit = self.edit.as_mut().unwrap();
        match event {
            Event::Quit { .. } | Event::Window { .. } => return ResultEvent::Pass,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                ..
            } => {
                self.cancel();
                *message = "Изменение отменено".into();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Return),
                repeat: false,
                ..
            } => {
                if self.purchase.is_none()
                    && self.training.is_none()
                    && state.entities.get(&edit.uid).is_some_and(|e| {
                        e["x"].as_i64() == Some(edit.x as i64 + 16)
                            && e["y"].as_i64() == Some(edit.y as i64 + 16)
                            && e["direction"].as_u64() == Some(edit.direction as u64)
                    })
                {
                    // Confirming the unchanged source position closes editing;
                    // it must not forge a move receipt or a quest event.
                    self.cancel();
                    return ResultEvent::Consumed;
                }
                return ResultEvent::Commit(edit.clone());
            }
            Event::KeyDown {
                keycode: Some(Keycode::R),
                repeat: false,
                ..
            } => {
                if let Some(id) = self.purchase.or_else(|| {
                    self.training.as_ref().and_then(|(_, id)| {
                        seed.workshop.as_ref()?.recipes.get(id).map(|r| r.output)
                    })
                }) {
                    edit.direction = world::next_direction(id, edit.direction);
                } else if let Err(text) = state.can_edit_world(seed, &edit.uid, true) {
                    *message = text;
                } else {
                    edit.direction = world::next_direction(
                        state.entities[&edit.uid]["type"].as_u64().unwrap() as u32,
                        edit.direction,
                    );
                }
            }
            Event::KeyDown {
                keycode: Some(k), ..
            } if !edit.rotation_only => match *k {
                Keycode::Left => edit.x = edit.x.saturating_sub(1),
                Keycode::Right => edit.x = edit.x.saturating_add(1),
                Keycode::Up => edit.y = edit.y.saturating_sub(1),
                Keycode::Down => edit.y = edit.y.saturating_add(1),
                _ => {}
            },
            Event::MouseMotion { x, y, .. }
                if !edit.rotation_only
                    && *y >= 78
                    && *y
                        < camera.height as i32 - warzone_app_client::hud::bottom_reserved(seed) =>
            {
                let p = camera.world([*x as f64, *y as f64]);
                let p = view.projection.inverse(p[0], p[1]);
                edit.x = p[0].round().clamp(-16.0, 111.0) as i32;
                edit.y = p[1].round().clamp(-16.0, 111.0) as i32;
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                ..
            } => self.pressed = true,
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } if std::mem::take(&mut self.pressed) => {
                if seed.interface.is_none()
                    && *y >= camera.height as i32 - 44
                    && (200..430).contains(x)
                {
                    self.cancel();
                    *message = "Изменение отменено".into();
                } else if (*y >= 78
                    && *y < camera.height as i32 - warzone_app_client::hud::bottom_reserved(seed))
                    || (seed.interface.is_none()
                        && *y >= camera.height as i32 - 44
                        && (16..186).contains(x))
                {
                    return ResultEvent::Commit(edit.clone());
                }
            }
            _ => {}
        }
        ResultEvent::Consumed
    }
    /// Current body grab point for real-pointer diagnostics, never a save edit.
    pub fn drag_geometry(
        &self,
        seed: &Scene,
        view: &Scene,
        state: &State,
        camera: &Camera,
    ) -> Option<([f64; 2], [f64; 2])> {
        let (e, _, _) = self.preview(seed, view, state)?;
        let part = e
            .tracks
            .iter()
            .flat_map(|t| t.frames.first().into_iter().flatten())
            .max_by(|a, b| (a.rect[2] * a.rect[3]).total_cmp(&(b.rect[2] * b.rect[3])))?;
        let origin = view.projection.project(e.x, e.y);
        Some((
            camera.screen([
                origin[0] + part.rect[0] + part.rect[2] / 2.,
                origin[1] + part.rect[1] + part.rect[3] / 2.,
            ]),
            camera.screen(origin),
        ))
    }
    pub fn preview(
        &self,
        seed: &Scene,
        view: &Scene,
        state: &State,
    ) -> Option<(Entity, bool, String)> {
        let edit = self.edit.as_ref()?;
        if let Some(id) = self.purchase.or_else(|| {
            self.training
                .as_ref()
                .and_then(|(_, id)| seed.workshop.as_ref()?.recipes.get(id).map(|r| r.output))
        }) {
            let e = state
                .building_preview(seed, id, edit.x, edit.y, edit.direction)
                .ok()?;
            let error = state
                .placement_type(seed, id, "", edit.x, edit.y, edit.direction)
                .err();
            return Some((
                e,
                error.is_none(),
                error.unwrap_or_else(|| {
                    "Место свободно · Enter: разместить и начать строительство".into()
                }),
            ));
        }
        let mut e = view.entities.iter().find(|e| e.uid == edit.uid)?.clone();
        e.x = edit.x as f64;
        e.y = edit.y as f64;
        if e.direction != edit.direction {
            if let Some(tracks) = seed
                .world
                .as_ref()?
                .catalog
                .get(&e.type_id)?
                .variants
                .get(&edit.direction)
            {
                e.tracks = tracks.clone();
            }
            e.direction = edit.direction;
        }
        if let Some(tracks) = state.upgraded_tracks(seed, e.type_id, edit.direction) {
            e.tracks = tracks.clone();
        }
        let error = state.placement(seed, edit).err();
        Some((
            e,
            error.is_none(),
            error.unwrap_or_else(|| {
                "Место свободно · Enter или ЛКМ: сохранить · Esc: отмена".into()
            }),
        ))
    }
}
