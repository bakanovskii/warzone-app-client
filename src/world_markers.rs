//! Original SP production/construction overlays, drawn between world and HUD.
use crate::{
    Camera, Scene,
    interface::{Action, Hit, View},
    production::{State, number},
};

/// SceneObject.iconMargin, source logical points. Horizontal margin is not
/// zoomed; Building.moveTooltip multiplies the vertical margin by world zoom.
pub fn margin(id: u32) -> [f64; 2] {
    match id {
        1005 => [0., -25.],
        1006 => [-15., -35.],
        1016..=1018 | 1035 => [-15., -25.],
        1000 | 1003 | 1007 | 1010 | 1019 | 1020 | 1026 | 1027 | 1034 => [0., -30.],
        1021 | 1022 | 1036 | 2000..=2999 => [0., -20.],
        _ => [0., 0.],
    }
}
fn timer(n: i64) -> String {
    if n >= 86400 {
        format!("{} д {}:{:02}", n / 86400, n / 3600 % 24, n / 60 % 60)
    } else if n >= 3600 {
        format!("{}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60)
    } else {
        format!("{}:{:02}", n / 60, n % 60)
    }
}
struct Work {
    remaining: i64,
    ready: bool,
    ready_key: &'static str,
    caption: String,
    icon: Option<(u32, f64)>,
}
fn work(
    scene: &Scene,
    state: &State,
    uid: &str,
    e: &serde_json::Value,
    now: i64,
    with_caption: bool,
) -> Option<Work> {
    if state.gathering(uid) {
        return None;
    }
    if e["isDeleted"] == true || e["isInBattle"] == true {
        return None;
    }
    let id = number(e, "type") as u32;
    // Readiness probes need no localized text or formatting.
    let caption = || {
        if with_caption {
            scene.item_name(id, "").to_owned()
        } else {
            String::new()
        }
    };
    let status = number(e, "state");
    if status == 0
        && scene
            .lifecycle
            .as_ref()
            .is_some_and(|c| c.repairs.contains_key(&id))
    {
        return Some(Work {
            remaining: 0,
            ready: true,
            ready_key: "world_ready_repair",
            caption: caption(),
            icon: None,
        });
    }
    if [1, 2].contains(&status)
        && scene
            .construction
            .as_ref()
            .is_some_and(|c| c.buildings.contains_key(&id))
    {
        let end = number(e, "buildStartTime").saturating_add(number(e, "buildTotalTime"));
        return Some(Work {
            remaining: end - now,
            ready: now >= end,
            ready_key: "world_ready_building",
            caption: caption(),
            icon: None,
        });
    }
    if status != 3 || number(e, "productionType") == 0 {
        return None;
    }
    let recipe = number(e, "productionType") as u32;
    let started = number(e, "productionStartTime");
    if id == 1019 {
        let r = scene.upgrades.as_ref()?.items.get(&recipe)?;
        let remaining = started.saturating_add(r.seconds) - now;
        return Some(Work {
            remaining,
            ready: remaining < 0,
            ready_key: "world_ready_upgrade",
            caption: if with_caption {
                crate::interface::lang(scene, &format!("#upgrade_{recipe}"), &r.name).into()
            } else {
                String::new()
            },
            icon: Some((recipe, f32::from_bits(0x3ee66666) as f64)),
        });
    }
    if id == 1020 {
        let r = scene.research.as_ref()?.items.get(&recipe)?;
        let remaining = started.saturating_add(r.seconds) - now;
        return Some(Work {
            remaining,
            ready: remaining < 0,
            ready_key: "world_ready_research",
            caption: if with_caption {
                crate::interface::lang(scene, &format!("#research_{recipe}"), &r.name).into()
            } else {
                String::new()
            },
            icon: Some((recipe, f32::from_bits(0x3f19999a) as f64)),
        });
    }
    if let Some(r) = state.workshop_choice(scene, uid, 0) {
        let remaining = started.saturating_add(r.seconds) - now;
        return Some(Work {
            remaining,
            ready: remaining < 0,
            ready_key: if r.unit() {
                if id == 1006 {
                    "world_ready_jeep"
                } else {
                    "world_ready_unit"
                }
            } else {
                "world_ready_material"
            },
            caption: if !with_caption {
                String::new()
            } else if r.unit() {
                caption()
            } else {
                scene.item_name(r.output, &r.name).into()
            },
            icon: if r.unit() {
                None
            } else {
                Some((recipe, f32::from_bits(0x3f333333) as f64))
            },
        });
    }
    let ec = scene.economy.as_ref()?;
    let r = ec.recipes.get(&recipe).filter(|r| {
        ec.buildings
            .get(&id)
            .is_some_and(|b| b.recipes.contains(&r.id))
    })?;
    let remaining = started.saturating_add(r.seconds) - now;
    let key = if r.outputs.contains_key("iron") {
        "world_ready_iron"
    } else if r.outputs.contains_key("fuel") {
        "world_ready_fuel"
    } else {
        "world_ready_playMoney"
    };
    Some(Work {
        remaining,
        ready: remaining < 0,
        ready_key: key,
        caption: caption(),
        icon: None,
    })
}
#[derive(Default)]
pub struct Playback {
    started: std::collections::BTreeMap<String, f64>,
    gathers: std::collections::BTreeMap<String, f64>,
}
impl Playback {
    pub fn update(&mut self, scene: &Scene, state: &State, now: i64, seconds: f64) {
        self.update_ms(scene, state, now * 1000, seconds);
    }
    pub fn update_ms(&mut self, scene: &Scene, state: &State, now_ms: i64, seconds: f64) {
        let now = now_ms / 1000;
        let pending = gather_times(state);
        self.gathers.retain(|uid, _| pending.contains_key(uid));
        for (uid, at) in pending {
            let elapsed = (now_ms - gather_millis(state, &uid, at)).max(0) as f64 / 1000.;
            self.gathers.entry(uid).or_insert(seconds - elapsed);
        }
        // One readiness pass, shared by retaining and admitting bob animations.
        // Retaining start times preserves the source two-second phase.
        let ready: std::collections::BTreeSet<_> = state
            .entities
            .iter()
            .filter_map(|(uid, e)| {
                work(scene, state, uid, e, now, false)
                    .filter(|w| w.ready)
                    .map(|_| uid.as_str())
            })
            .collect();
        self.started.retain(|uid, _| ready.contains(uid.as_str()));
        for uid in ready {
            self.started.entry(uid.into()).or_insert(seconds);
        }
    }
    pub fn offset(&self, uid: &str, seconds: f64) -> f64 {
        let Some(start) = self.started.get(uid) else {
            return 0.;
        };
        let phase = (seconds - start).max(0.) % 2.;
        // createIconFinish/createIconRepair: SPTween1s, linear, loop2. Source
        // SPTween.advanceTime reverses alternate loops. Position0 ->10 ->0.
        10. * if phase <= 1. { phase } else { 2. - phase }
    }
    fn gather_elapsed(&self, uid: &str, seconds: f64, fallback: f64) -> f64 {
        self.gathers
            .get(uid)
            .map_or(fallback, |at| (seconds - at).max(0.))
    }
}
pub fn build(scene: &Scene, state: &State, camera: &Camera, now: i64) -> View {
    build_animated(scene, state, camera, now, None, 0.)
}
/// Building.onTouch uses the same strict production / inclusive construction
/// boundary as the marker. Progress tooltips themselves are setTouchable0.
pub fn is_ready(scene: &Scene, state: &State, uid: &str, now: i64) -> bool {
    state
        .entities
        .get(uid)
        .and_then(|e| work(scene, state, uid, e, now, false))
        .is_some_and(|w| w.ready)
}
pub fn build_animated(
    scene: &Scene,
    state: &State,
    camera: &Camera,
    now: i64,
    playback: Option<&Playback>,
    seconds: f64,
) -> View {
    build_selected(scene, state, camera, now, playback, seconds, None, false)
}
pub fn build_selected(
    scene: &Scene,
    state: &State,
    camera: &Camera,
    now: i64,
    playback: Option<&Playback>,
    seconds: f64,
    selected: Option<&str>,
    only_selected: bool,
) -> View {
    let mut v = View::default();
    let Some(c) = &scene.interface else {
        return v;
    };
    if !c.symbols.contains_key("world_working") {
        return v;
    }
    let scale = crate::hud::scale(camera);
    let gathers = gather_times(state);
    for (index, (uid, e)) in state.entities.iter().enumerate() {
        if let Some(pending) = gathers.get(uid) {
            if let Some(symbol) = c.symbols.get("world_gathering") {
                let anchor = camera.screen(
                    scene
                        .projection
                        .project((number(e, "x") - 16) as f64, (number(e, "y") - 16) as f64),
                );
                let offset = margin(number(e, "type") as u32);
                let o = [
                    anchor[0] / scale + offset[0],
                    anchor[1] / scale + offset[1] * camera.zoom,
                ];
                let at = v.draws.len();
                v.add(
                    symbol,
                    o,
                    scale,
                    |_| true,
                    |n| {
                        (n.id == "caption_txt")
                            .then(|| crate::interface::lang(scene, "#collect_hint", "").into())
                    },
                    None,
                );
                let elapsed = (now - *pending).max(0) as f64;
                let duration = state
                    .pending_production_gathers()
                    .ok()
                    .and_then(|p| {
                        p.get(uid).map(|p| {
                            if p.at_ms.is_some() {
                                crate::production_gather::SOURCE_SECONDS
                            } else {
                                crate::production_gather::SECONDS
                            }
                        })
                    })
                    .unwrap_or(crate::upgrade_gather::SECONDS);
                let progress = (playback
                    .map_or(elapsed, |p| p.gather_elapsed(uid, seconds, elapsed))
                    / duration as f64)
                    .clamp(0., 1.);
                let rect = symbol
                    .frames
                    .get("progressbar")
                    .or_else(|| symbol.slots.get("progressbar"));
                for d in &mut v.draws[at..] {
                    if d.node.id == "progressbar"
                        || d.node.parents.iter().any(|p| p == "progressbar")
                    {
                        if progress == 0. {
                            d.progress = 0.;
                        }
                        if let Some(r) = rect {
                            d.clip = Some([
                                (o[0] + r[0]) * scale,
                                (o[1] + r[1]) * scale,
                                r[2] * scale * progress,
                                r[3] * scale,
                            ]);
                        }
                    }
                    d.node.id = format!("world:{uid}:gather:{}", d.node.id);
                }
                // Building.createTooltipGather explicitly setsTouchable0.
            }
            continue;
        }
        let Some(work) = work(scene, state, uid, e, now, true) else {
            continue;
        };
        let ready = work.ready;
        let construction = matches!(number(e, "state"), 1 | 2);
        // Buildings.isShowProductionWithIcon: these three jobs keep their
        // product tooltip independently of the selected map object.
        let persistent_product = matches!(number(e, "type"), 1019 | 1020 | 1038);
        if !ready
            && !construction
            && !persistent_product
            && only_selected
            && selected != Some(uid.as_str())
        {
            continue;
        }
        let Some(symbol) = c.symbols.get(if ready {
            work.ready_key
        } else {
            "world_working"
        }) else {
            continue;
        };
        let anchor = camera.screen(
            scene
                .projection
                .project((number(e, "x") - 16) as f64, (number(e, "y") - 16) as f64),
        );
        let offset = margin(number(e, "type") as u32);
        let bob = if ready {
            playback.map_or(0., |p| p.offset(uid, seconds))
        } else {
            0.
        };
        // Ready bubbles belong to the world parent. Keep their authored size
        // at the default view, and shrink with the scene when zooming out.
        let marker_scale = if ready {
            scale * (camera.zoom / scene.camera[2]).min(1.)
        } else {
            crate::interface_unit_health::scale(camera)
        };
        let o = [
            anchor[0] / marker_scale
                + offset[0]
                + if ready && work.ready_key != "world_ready_repair" {
                    5.
                } else {
                    0.
                },
            anchor[1] / marker_scale + offset[1] * camera.zoom * scale / marker_scale + bob,
        ];
        let text = timer(work.remaining.max(0));
        let short = text.len() < 6;
        let start = v.draws.len();
        let icon = work
            .icon
            .and_then(|(id, factor)| c.work_icons.get(&id).map(|s| (s, factor)));
        v.add(
            symbol,
            o,
            marker_scale,
            |n| {
                if ready {
                    return true;
                }
                !n.parents.iter().any(|p| p == "icon_place")
                    && match n.id.as_str() {
                        "clock_icon" => icon.is_none(),
                        "timer_short_txt" => short,
                        "timer_txt" => !short,
                        _ => true,
                    }
            },
            |n| {
                Some(match n.id.as_str() {
                    "caption_txt" => work.caption.clone(),
                    "timer_txt" | "timer_short_txt" => text.clone(),
                    _ => String::new(),
                })
            },
            None,
        );
        if !ready {
            if let Some((icon, factor)) = icon {
                let slot = &symbol.slots["icon_place"];
                v.add(
                    icon,
                    [(o[0] + slot[0]) / factor, (o[1] + slot[1]) / factor],
                    marker_scale * factor,
                    |_| true,
                    |_| None,
                    None,
                );
            }
        }
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for d in &mut v.draws[start..] {
            d.node.id = format!("world:{uid}:{}", d.node.id);
            bounds[0] = bounds[0].min(d.node.rect[0]);
            bounds[1] = bounds[1].min(d.node.rect[1]);
            bounds[2] = bounds[2].max(d.node.rect[0] + d.node.rect[2]);
            bounds[3] = bounds[3].max(d.node.rect[1] + d.node.rect[3]);
        }
        if bounds[2] < 0.
            || bounds[0] > camera.width as f64
            || bounds[3] < 0.
            || bounds[1] > camera.height as f64
        {
            v.draws.truncate(start);
            continue;
        }
        if ready {
            let x = bounds[0].max(0.);
            let y = bounds[1].max(crate::hud::height(scene, camera) as f64);
            let right = bounds[2].min(camera.width as f64);
            let bottom = bounds[3].min(camera.height as f64);
            if right > x && bottom > y {
                v.hits.push(Hit {
                    action: Action::ObjectPrimary(index),
                    rect: [x, y, right - x, bottom - y],
                });
            }
        }
    }
    v
}

fn gather_times(state: &State) -> std::collections::BTreeMap<String, i64> {
    let mut times: std::collections::BTreeMap<_, _> = state
        .pending_upgrade_gathers()
        .unwrap_or_default()
        .into_iter()
        .map(|(uid, p)| (uid, p.at))
        .collect();
    times.extend(
        state
            .pending_production_gathers()
            .unwrap_or_default()
            .into_iter()
            .map(|(uid, p)| (uid, p.at)),
    );
    times
}

fn gather_millis(state: &State, uid: &str, fallback: i64) -> i64 {
    state
        .pending_upgrade_gathers()
        .ok()
        .and_then(|q| q.get(uid).and_then(|p| p.at_ms))
        .or_else(|| {
            state
                .pending_production_gathers()
                .ok()
                .and_then(|q| q.get(uid).and_then(|p| p.at_ms))
        })
        .unwrap_or(fallback * 1000)
}
