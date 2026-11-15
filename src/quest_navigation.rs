//! Read-only target resolution for QuestDialog.onButtonShowExercise and Tutorials.
use crate::{
    Scene,
    production::{State, number},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Options {
    pub button: bool,
    pub quick: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub exercises: BTreeMap<u32, Vec<Options>>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Building { id: u32, quick: bool },
    Production { building: u32, recipe: u32 },
    Mission(u32),
    Expansion(u32),
    Research(u32),
    Upgrade(u32),
    Pvp,
    ClearJunk,
    Speedup,
}

/// Automatic, read-only steps before the first admitted tutorial touch.
/// Tutorials.miniTutorialStartForQuest (0x139a5d) stops at a sole unfinished
/// producer; PrepareToCollectEntity/StartRecipe open the busy dialog only
/// after the shared world scroll completes. No timer or purchase is forced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    World,
    OpenBusy,
    ScrollOnly,
}
pub fn entry(
    scene: &Scene,
    state: &State,
    requested: &Route,
    uid: Option<&str>,
    now: i64,
) -> Entry {
    let Some(uid) = uid else { return Entry::World };
    let Some(e) = state.entities.get(uid) else {
        return Entry::World;
    };
    let producer = match requested {
        Route::Production { building, .. } => Some(*building),
        Route::Research(_) => Some(1020),
        Route::Upgrade(_) => Some(1019),
        _ => None,
    };
    if let Some(id) = producer {
        let count = state
            .entities
            .values()
            .filter(|e| e["type"] == id && e["isDeleted"] != true && e["isInBattle"] != true)
            .count();
        if count == 1 && number(e, "state") != 3 {
            return Entry::ScrollOnly;
        }
    }
    // Research/upgrade busy branches contain only the world scroll, with
    // no follow-up hole, even when the active timer is ready to collect.
    if matches!(requested, Route::Research(_) | Route::Upgrade(_))
        && number(e, "productionType") != 0
    {
        return Entry::ScrollOnly;
    }
    // Another contract is only a scroll when already ready. PrepareToStartRecipe
    // reserves a world hole for the requested contract, and appends an automatic
    // busy dialog + speedup only while the other contract's timer is running.
    let running = scene
        .economy
        .as_ref()
        .and_then(|c| now.checked_sub(c.time_shift))
        .and_then(|wall| state.production_speedup_quote(scene, uid, wall).ok())
        .is_some_and(|q| q.remaining > 0);
    if matches!(requested,Route::Production{recipe,..} if number(e,"productionType")!=0 && number(e,"productionType")!=*recipe as i64)
        && !running
    {
        return Entry::ScrollOnly;
    }
    // Ordinary production/building busy branches automatically open a dialog.
    if matches!(
        requested,
        Route::Building { .. } | Route::Production { .. } | Route::Speedup
    ) && running
    {
        Entry::OpenBusy
    } else {
        Entry::World
    }
}
/// Last successful commands in the recovered PrepareTo* step lists. This
/// does not award an objective or change the save; it only ends one UI guide.
pub fn finishes(route: &Route, uid: Option<&str>, a: &crate::production::Action) -> bool {
    let same = uid == Some(a.uid.as_str());
    match route {
        Route::Building { id, .. } => {
            same && a.recipe == *id
                && matches!(a.kind.as_str(), "building_finish" | "building_speedup")
        }
        Route::Production { recipe, .. } => {
            same && (a.recipe == *recipe
                && matches!(
                    a.kind.as_str(),
                    "start" | "workshop_start" | "gather_resource" | "gather_material"
                )
                || matches!(a.kind.as_str(), "resource_speedup" | "workshop_speedup"))
                || a.kind == "unit_train"
                    && a.recipe == *recipe
                    && a.source
                        .as_ref()
                        .is_some_and(|s| uid == Some(s.uid.as_str()))
        }
        Route::Research(id) => {
            same && a.recipe == *id
                && matches!(
                    a.kind.as_str(),
                    "research_start" | "gather_research" | "research_buy"
                )
        }
        Route::Upgrade(id) => {
            same && a.recipe == *id
                && matches!(
                    a.kind.as_str(),
                    "upgrade_start" | "gather_upgrade" | "gather_upgrade_flight" | "upgrade_buy"
                )
        }
        Route::Mission(id) => a.kind == "mission_open" && a.recipe == *id,
        Route::Pvp => a.kind == "mission_open" && a.recipe == 5990,
        Route::Expansion(id) => {
            a.recipe == *id
                && matches!(
                    a.kind.as_str(),
                    "land_buy_coins" | "land_buy_cash" | "land_buy_shortfall"
                )
        }
        Route::ClearJunk => same && a.kind == "obstacle_clear",
        Route::Speedup => {
            same && matches!(
                a.kind.as_str(),
                "building_speedup" | "resource_speedup" | "workshop_speedup"
            )
        }
    }
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        let quests = &scene
            .progression
            .as_ref()
            .ok_or("Missing quest config")?
            .quests;
        if self.exercises.len() != quests.len()
            || quests.iter().any(|q| {
                self.exercises
                    .get(&q.id)
                    .is_none_or(|e| e.len() != q.exercises.len())
            })
        {
            return Err("Quest navigation metadata mismatch".into());
        }
        Ok(())
    }
}
fn building_for(scene: &Scene, recipe: u32) -> Option<u32> {
    scene
        .economy
        .as_ref()
        .and_then(|c| {
            c.buildings
                .iter()
                // Crystal fields expose the worker recipe in the source XML,
                // but onProductionTouch belongs to the harvester at that field.
                .find(|(id, b)| !matches!(**id, 1026 | 1028) && b.recipes.contains(&recipe))
                .map(|(id, _)| *id)
        })
        .or_else(|| {
            scene.workshop.as_ref().and_then(|c| {
                c.buildings
                    .iter()
                    .find(|(_, rs)| rs.contains(&recipe))
                    .map(|(id, _)| *id)
            })
        })
}
pub fn visible(scene: &Scene, state: &State, quest: u32, index: usize) -> bool {
    let Some(q) = state
        .active_quests(scene)
        .into_iter()
        .find(|q| q.id == quest)
    else {
        return false;
    };
    let Some(e) = q.exercises.get(index) else {
        return false;
    };
    let options = scene
        .quest_navigation
        .as_ref()
        .and_then(|c| c.exercises.get(&quest))
        .and_then(|es| es.get(index));
    options.is_some_and(|o| o.button)
        && !state.exercise_progress(scene, q, index).1
        && (e.kind != "start_research"
            || state
                .entities
                .values()
                .any(|v| v["type"] == 1020 && v["isDeleted"] != true))
        && route(scene, state, quest, index).is_some()
}
pub fn route(scene: &Scene, state: &State, quest: u32, index: usize) -> Option<Route> {
    let q = state
        .active_quests(scene)
        .into_iter()
        .find(|q| q.id == quest)?;
    let e = q.exercises.get(index)?;
    let opt = scene
        .quest_navigation
        .as_ref()?
        .exercises
        .get(&quest)?
        .get(index)?;
    if !opt.button || state.exercise_progress(scene, q, index).1 {
        return None;
    }
    let recipe = match e.kind.as_str() {
        "collect_fuel" if e.item_id == 0 => 1008,
        "collect_iron" if e.item_id == 0 => 1000,
        "collect_playmoney" if e.item_id == 0 => 1016,
        _ => e.item_id,
    };
    Some(match e.kind.as_str() {
        "collect_entity" if (1000..2000).contains(&e.item_id) => Route::Building {
            id: e.item_id,
            quick: opt.quick,
        },
        "collect_entity" if (2000..3000).contains(&e.item_id) => Route::Production {
            building: building_for(scene, e.item_id)?,
            recipe: e.item_id,
        },
        "start_recipe" | "collect_fuel" | "collect_iron" | "collect_playmoney" => {
            Route::Production {
                building: building_for(scene, recipe)?,
                recipe,
            }
        }
        "start_mission" | "complete_mission" | "order_item_for_mission" => {
            Route::Mission(e.item_id)
        }
        "buy_expansion" => Route::Expansion(e.item_id),
        "start_research" | "collect_research" => Route::Research(e.item_id),
        "collect_upgrade" => Route::Upgrade(e.item_id),
        "clear_junk" => Route::ClearJunk,
        "show_speedup_tutorial" => Route::Speedup,
        "collect_pvp_playmoney" | "collect_pvp_fuel" | "collect_pvp_iron" => Route::Pvp,
        _ => return None,
    })
}
pub fn object(state: &State, building: u32, recipe: Option<u32>, unfinished: bool) -> Option<&str> {
    state
        .entities
        .iter()
        .filter(|(_, e)| {
            e["type"] == building
                && e["isDeleted"] != true
                && e["isInBattle"] != true
                && (!unfinished || number(e, "state") != 3)
        })
        .min_by_key(|(_, e)| match recipe {
            Some(id) if e["productionType"] == id => 0,
            Some(_) if e["productionType"] == 0 && e["state"] == 3 => 1,
            Some(_) => 2,
            None => 0,
        })
        .map(|(uid, _)| uid.as_str())
}

/// Resolve again after each live scene revision. A missing producer may have
/// just been bought, completed, sold or replaced by a different contract. The
/// source tutorialFindActiveProduction/FreeBuilding/AnotherContracts queries
/// inspect live objects at each step, never a captured opening-time UID.
pub fn target(scene: &Scene, state: &State, route: Route) -> (Route, Option<String>) {
    let wanted = match &route {
        Route::Building { id, .. } => Some((*id, None, true)),
        Route::Production { building, recipe } => Some((*building, Some(*recipe), false)),
        Route::Research(_) => Some((1020, None, false)),
        Route::Upgrade(_) => Some((1019, None, false)),
        _ => None,
    };
    if let Some((id, recipe, unfinished)) = wanted {
        if let Some(uid) = object(state, id, recipe, unfinished) {
            return (route, Some(uid.into()));
        }
        return (
            Route::Building {
                id,
                quick: matches!(route, Route::Building { quick: true, .. }),
            },
            None,
        );
    }
    if route == Route::ClearJunk {
        let uid = scene
            .entities
            .iter()
            .find(|e| {
                e.kind == "obstacles"
                    && state.entities[&e.uid]["isDeleted"] != true
                    && crate::expansions::owns_point(state, e.x as i32, e.y as i32)
            })
            .map(|e| e.uid.clone());
        return (route, uid);
    }
    (route, None)
}

pub fn draw_arrow(
    view: &mut crate::interface::View,
    symbol: &crate::interface::Symbol,
    origin: [f64; 2],
    scale: f64,
) {
    view.add(symbol, origin, scale, |_| true, |_| None, None);
}

/// MiniTutorialItem.initWithType defaults to100x50 phone points. Explicit world
/// holes are multiplied by SceneMain.getZoom (background scale), not HUD scale.
/// Our world projection uses the original2x bitmap grid, so native camera.zoom
/// already includes that factor; multiplying a world hole by HUD scale again
/// made the research/junk focus twice as large at960x640.
pub fn world_hole(route: &Route, active: bool) -> Option<([f64; 2], f64)> {
    Some(match *route {
        Route::ClearJunk => ([200., 120.], -12.),
        Route::Research(_) | Route::Upgrade(_) => ([400., 280.], -24.),
        Route::Building { id: 1027, .. } => ([400., 240.], 0.),
        Route::Building { id: 1005, .. } => ([350., 240.], 0.),
        Route::Building {
            id: 1016 | 1035, ..
        } => ([300., 200.], -8.),
        Route::Building { id: 1007, .. } => ([260., 144.], -8.),
        Route::Building { .. } => ([240., 176.], 0.),
        Route::Production { building, .. } => {
            if active {
                match building {
                    1027 => ([400., 240.], 0.),
                    1005 | 1019 | 1020 => ([360., 224.], 0.),
                    _ => ([240., 176.], 0.),
                }
            } else {
                match building {
                    1005 | 1006 | 1019 | 1020 | 1038 => ([360., 224.], -8.),
                    _ => ([180., 104.], 0.),
                }
            }
        }
        _ => return None,
    })
}
/// miniTutorialShowHole averages the projected opposite footprint corners,
/// using integer realHeight/2 and realWidth/2. Odd footprints have a half-cell
/// displacement; this is different from sortAllObjects' (size-1) corners.
pub fn world_center(
    scene: &Scene,
    state: &State,
    camera: &crate::Camera,
    uid: &str,
) -> Option<[f64; 2]> {
    let e = state.entities.get(uid)?;
    let id = e.get("type")?.as_u64()? as u32;
    let d = scene.world.as_ref()?.catalog.get(&id)?;
    let (w, h) = if [crate::world::SW, crate::world::NE].contains(&(number(e, "direction") as u32))
    {
        (d.height, d.width)
    } else {
        (d.width, d.height)
    };
    Some(camera.screen(scene.projection.project(
        number(e, "x") as f64 - 16. + w / 2. - (w / 2.).trunc(),
        number(e, "y") as f64 - 16. + h / 2. - (h / 2.).trunc(),
    )))
}
/// Original helper0x136f1d: UIView Repeat|Autoreverse|Linear, duration0.5s.
pub fn arrow_phase(seconds: f64) -> f64 {
    let p = seconds.max(0.) % 1.;
    if p <= 0.5 { p * 2. } else { 2. - p * 2. }
}
/// Tutorials.showArrowAnimated0x1373e9: position(.9*time), compression and
/// recovery(.45*time each), then position back(.9*time). time=.5s.
pub fn tap_pose(seconds: f64) -> (f64, f64) {
    let t = seconds.max(0.) % 1.35;
    if t < 0.45 {
        (curve(t / 0.45, true), 0.)
    } else if t < 0.675 {
        (1., curve((t - 0.45) / 0.225, false))
    } else if t < 0.9 {
        (1., 1. - curve((t - 0.675) / 0.225, true))
    } else {
        (1. - curve((t - 0.9) / 0.45, false), 0.)
    }
}
fn curve(t: f64, inward: bool) -> f64 {
    let (x1, x2) = if inward { (0.42, 1.) } else { (0., 0.58) };
    let mut lo: f64 = 0.;
    let mut hi: f64 = 1.;
    for _ in 0..20 {
        let p = (lo + hi) / 2.;
        let x = 3. * (1. - p).powi(2) * p * x1 + 3. * (1. - p) * p * p * x2 + p * p * p;
        if x < t { lo = p } else { hi = p }
    }
    let p = (lo + hi) / 2.;
    3. * (1. - p) * p * p + p * p * p
}

/// Source Tutorials.showHoleInPosX:andY:width:height: uses a black
/// UIViewWithHole at alpha.25 and a red focus outline. This is cosmetic;
/// quest counters and the user's selected producer remain in the model.
pub fn draw_focus(
    view: &mut crate::interface::View,
    template: &crate::interface::Node,
    rect: [f64; 4],
) {
    let mut node = template.clone();
    node.id = "source_quest_focus".into();
    node.kind = "tutorial_mask".into();
    node.rect = rect;
    node.texture.clear();
    node.text.clear();
    node.parents.clear();
    node.color = 0;
    view.draws.push(crate::interface::Draw {
        node: node.clone(),
        clip: None,
        progress: 1.,
        opacity: 64,
        grayscale: false,
    });
    node.kind = "tutorial_ring".into();
    node.color = 0xff0000;
    view.draws.push(crate::interface::Draw {
        node,
        clip: None,
        progress: 1.,
        opacity: 255,
        grayscale: false,
    });
}

/// SceneMain.tutorialScrollToTileX:andY:showHole:addAfterScroll: (0xa3009).
/// The source moves all three world parents with the same cubic easeOut tween;
/// duration is phone-point distance /200, capped at one second.
#[derive(Clone, Debug)]
pub struct CameraMotion {
    pub from: [f64; 2],
    pub target: [f64; 2],
    pub duration: f64,
}
impl CameraMotion {
    pub fn new(camera: &crate::Camera, target: [f64; 2]) -> Self {
        let scale = crate::hud::scale(camera);
        let dx = ((target[0] - camera.x) * camera.zoom / scale) as f32;
        let dy = ((target[1] - camera.y) * camera.zoom / scale) as f32;
        let duration = (((dx * dx + dy * dy).sqrt() / 200.) as f64).min(1.);
        Self {
            from: [camera.x, camera.y],
            target,
            duration,
        }
    }
    pub fn sample(&self, seconds: f64) -> ([f64; 2], bool) {
        let t = if self.duration <= 0. {
            1.
        } else {
            (seconds / self.duration).clamp(0., 1.)
        };
        let progress = 1. + (t - 1.).powi(3);
        (
            [
                self.from[0] + (self.target[0] - self.from[0]) * progress,
                self.from[1] + (self.target[1] - self.from[1]) * progress,
            ],
            t >= 1.,
        )
    }
}

/// UIViewWithHole.hitTest0x141665: the visual cutout is an ellipse, but
/// input deliberately passes through its rectangular bounds, right/bottom open.
pub fn focus_contains(rect: [f64; 4], point: [f64; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}
#[derive(Clone, Debug, PartialEq)]
pub enum CancelButton {
    Hidden,
    Visible([f64; 4]),
    Abandon,
}
/// Tutorials.showHole.../checkCloseButton: hidden until3outside touch ends or
///10game seconds. Three early outside taps rescue an offscreen close to center;
/// timeout with an offscreen button cancels the guide instead.
pub fn cancel_button(
    hole: [f64; 4],
    size: [f64; 2],
    viewport: [f64; 2],
    outside_taps: u32,
    elapsed: i64,
) -> CancelButton {
    if outside_taps < 3 && elapsed < 10 {
        return CancelButton::Hidden;
    }
    let mut center = [hole[0] + hole[2], hole[1]];
    if center[0] >= viewport[0] - size[0] * 0.5 {
        center[0] -= hole[2];
    }
    if center[1] <= size[1] * 0.5 {
        center[1] += hole[3];
    }
    if center[0] <= 0. || center[0] >= viewport[0] || center[1] <= 0. || center[1] >= viewport[1] {
        if outside_taps >= 3 {
            center = [viewport[0] * 0.5, viewport[1] * 0.5];
        } else {
            return CancelButton::Abandon;
        }
    }
    CancelButton::Visible([
        center[0] - size[0] * 0.5,
        center[1] - size[1] * 0.5,
        size[0],
        size[1],
    ])
}
/// Reuse source marker bitmap and its original width-based affine transform.
/// Legacy packs keep the accepted vector ring instead of inventing an asset.
pub fn draw_source_focus(
    view: &mut crate::interface::View,
    template: &crate::interface::Node,
    rect: [f64; 4],
    ring: Option<&crate::interface::Symbol>,
) {
    draw_focus(view, template, rect);
    if let Some(ring) = ring.filter(|r| r.nodes.len() == 1) {
        let mut node = ring.nodes[0].clone();
        node.id = "source_quest_focus".into();
        // Tutorials.showHole... scales X and Y independently. Keeping the
        // image's aspect here turned a wide button focus into a large circle.
        let width = rect[2] * 1.05;
        let height = rect[3] * 1.05 * node.rect[3] / node.rect[2];
        node.rect = [
            rect[0] + (rect[2] - width) * 0.5,
            rect[1] + (rect[3] - height) * 0.5,
            width,
            height,
        ];
        view.draws.pop();
        view.draws.push(crate::interface::Draw {
            node,
            clip: None,
            progress: 1.,
            opacity: 255,
            grayscale: false,
        });
    }
}

pub fn draw_cancel(
    view: &mut crate::interface::View,
    button: &crate::interface::Symbol,
    rect: [f64; 4],
    hud_scale: f64,
) {
    let begin = view.draws.len();
    let scale = hud_scale;
    view.add(
        button,
        [rect[0] / scale, rect[1] / scale],
        scale,
        |_| true,
        |_| None,
        None,
    );
    for d in &mut view.draws[begin..] {
        d.node.id = "source_tutorial_cancel".into();
    }
    view.hits.push(crate::interface::Hit {
        action: crate::interface::Action::TutorialCancel,
        rect,
    });
    view.blocks.push(rect);
}
