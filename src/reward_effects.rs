//! Original reward art, cubic drop/bounce and flight. All animation state and
//! visual randomness are transient: no reward RNG or wallet is touched here.
use crate::{
    Camera, Scene,
    interface::View,
    production::{State, number},
};
#[derive(Clone)]
struct Effect {
    mission: Option<String>,
    lifetime: f64,
    instance: usize,
    drop_sound_pending: bool,
    position: [f64; 2],
    recipe: u32,
    sequence: u64,
    count: i64,
    cosmetic: bool,
    offset: [f64; 2],
    at: f64,
    fly: Option<f64>,
    path: Option<FlightPath>,
}
#[derive(Default)]
pub struct Playback {
    revision: Option<u64>,
    effects: Vec<Effect>,
    sounds: Vec<&'static str>,
    // CHUDLayer.updateMoney/Exp/Oil/Iron refresh text on animateFlight.
    // The durable wallet is already paid; this is presentation only.
    held: std::collections::BTreeMap<u32, i64>,
    context: Option<String>,
}
impl Playback {
    pub fn initialize(&mut self, revision: u64) {
        self.revision = Some(revision);
        self.effects.clear();
        self.sounds.clear();
        self.held.clear();
        self.context = None;
    }
    pub fn update(&mut self, scene: &Scene, state: &State, seconds: f64) {
        let Some(before) = self.revision else {
            self.initialize(state.revision);
            return;
        };
        let context = state
            .mission
            .as_ref()
            .filter(|_| !state.mission_paused())
            .map(|m| m.id.clone());
        if self.context != context {
            self.held.clear();
            self.context = context;
        }
        for a in state.actions.iter().filter(|a| a.sequence > before) {
            let cosmetic = matches!(
                a.kind.as_str(),
                "collect" | "workshop_collect" | "mission_move_bonus" | "mission_destroy_bonus"
            );
            if a.gains.get("level").is_some_and(|n| *n > 0) {
                self.held.clear();
            } else {
                for (id, key) in [(1, "playMoney"), (2, "exp"), (4, "fuel"), (5, "iron")] {
                    if let Some(n) = a.gains.get(key).filter(|n| **n != 0) {
                        if cosmetic
                            && *n > 0
                            && (a.kind != "mission_move_bonus"
                                || state
                                    .mission
                                    .as_ref()
                                    .filter(|_| !state.mission_paused())
                                    .is_some_and(|m| {
                                        a.source.as_ref().is_some_and(|s| s.uid == m.id)
                                            && m.battle.units.iter().any(|u| u.uid == a.uid)
                                    }))
                        {
                            *self.held.entry(id).or_default() += n;
                        } else {
                            self.held.remove(&id);
                        }
                    }
                }
            }

            if a.kind == "building_finish" {
                // SceneMain.addNewItemTouch_0x82f59 schedules one money icon
                // after0.4s and XP after0.5s. The receipt already contains BOTH
                // source credits; display the original award once, never pay again.
                if let Some(b) = scene
                    .construction
                    .as_ref()
                    .and_then(|c| c.buildings.get(&a.recipe))
                {
                    let mut words = a.entropy.iter();
                    for (kind, id, delay) in [("playMoney", 1, 0.4), ("experience", 2, 0.5)] {
                        for bonus in b
                            .awards
                            .iter()
                            .filter(|b| b.kind == kind && b.count != 0 && b.probability != 0)
                        {
                            let Some(word) = words.next() else { continue };
                            if ((word & 0x7fffffff) % 100) as i64 >= bonus.probability {
                                continue;
                            }
                            self.effects.push(Effect {
                                mission: None,
                                lifetime: 3.75,
                                instance: 0,
                                drop_sound_pending: true,
                                position: position(state, &a.uid),
                                recipe: id,
                                sequence: a.sequence,
                                count: bonus.count,
                                cosmetic: true,
                                offset: scatter(),
                                at: seconds + delay,
                                fly: None,
                                path: None,
                            });
                        }
                    }
                }
                continue;
            }
            if a.kind == "mission_destroy_bonus" {
                if let Some(m) = state
                    .mission
                    .as_ref()
                    .filter(|_| !state.mission_paused())
                    .filter(|m| a.source.as_ref().is_some_and(|s| s.uid == m.id))
                {
                    if let Some(u) = m.battle.units.iter().find(|u| u.uid == a.uid) {
                        let mut delay = 0.;
                        for (key, id) in [("exp", 2), ("playMoney", 1), ("fuel", 4), ("iron", 5)] {
                            if let Some(count) = a.gains.get(key).filter(|n| **n > 0) {
                                self.effects.push(Effect {
                                    instance: 0,
                                    drop_sound_pending: true,
                                    mission: Some(m.id.clone()),
                                    lifetime: 1.25,
                                    position: [u.x - 16., u.y - 16.],
                                    recipe: id,
                                    sequence: a.sequence,
                                    count: *count,
                                    cosmetic: true,
                                    offset: scatter(),
                                    at: seconds + delay,
                                    fly: None,
                                    path: None,
                                });
                                delay += 0.2;
                            }
                        }
                        for (key, count) in &a.gains {
                            if let Some(id) = key.strip_prefix("item:").and_then(|s| s.parse().ok())
                            {
                                self.effects.push(Effect {
                                    instance: 0,
                                    drop_sound_pending: true,
                                    mission: Some(m.id.clone()),
                                    lifetime: 1.25,
                                    position: [u.x - 16., u.y - 16.],
                                    recipe: id,
                                    sequence: a.sequence,
                                    count: *count,
                                    cosmetic: false,
                                    offset: scatter(),
                                    at: seconds + delay,
                                    fly: None,
                                    path: None,
                                });
                                delay += 0.2;
                            }
                        }
                    }
                }
                continue;
            }
            if a.kind == "mission_move_bonus" {
                if let Some(m) = state
                    .mission
                    .as_ref()
                    .filter(|_| !state.mission_paused())
                    .filter(|m| a.source.as_ref().is_some_and(|s| s.uid == m.id))
                {
                    if let Some(u) = m.battle.units.iter().find(|u| u.uid == a.uid) {
                        let delay = (crate::save::entropy().unwrap_or(0) % 10 / 5) as f64;
                        self.effects.push(Effect {
                            instance: 0,
                            drop_sound_pending: true,
                            mission: Some(m.id.clone()),
                            lifetime: if m.battle.tutorial.is_some() {
                                1.75
                            } else {
                                1.25
                            },
                            position: [u.x - 16., u.y - 16.],
                            recipe: 2,
                            sequence: a.sequence,
                            count: 1,
                            cosmetic: true,
                            offset: scatter(),
                            at: seconds + delay,
                            fly: None,
                            path: None,
                        });
                    }
                }
                continue;
            }
            if matches!(a.kind.as_str(), "upgrade_finish" | "research_finish")
                && state.actions.iter().any(|g| {
                    g.kind
                        == if a.kind == "research_finish" {
                            "gather_research"
                        } else {
                            "gather_upgrade_flight"
                        }
                        && g.recipe == a.recipe
                        && g.sequence < a.sequence
                })
            {
                self.effects.push(Effect {
                    mission: None,
                    lifetime: 3.75,
                    instance: 0,
                    drop_sound_pending: true,
                    position: position(state, &a.uid),
                    recipe: a.recipe,
                    sequence: a.sequence,
                    count: 1,
                    cosmetic: false,
                    offset: scatter(),
                    at: seconds,
                    fly: None,
                    path: None,
                });
            } else if matches!(
                a.kind.as_str(),
                "collect" | "workshop_collect" | "obstacle_bonus" | "unit_train"
            ) {
                // Currency/experience and item gains are already committed by
                // the source addFallingItem callback. Flight only updates their
                // presentation; reloading a save must never grant them again.
                // Source onProductionGatherStart0x89961 iterates XP, coins,
                // fuel, iron, then items. BTreeMap's lexical order is different.
                for (key, count) in ["exp", "playMoney", "fuel", "iron"]
                    .into_iter()
                    .filter_map(|key| a.gains.get_key_value(key))
                    .chain(a.gains.iter().filter(|(key, _)| key.starts_with("item:")))
                {
                    let id = match key.as_str() {
                        "playMoney" => 1,
                        "exp" => 2,
                        "fuel" => 4,
                        "iron" => 5,
                        k => match k.strip_prefix("item:").and_then(|n| n.parse().ok()) {
                            Some(id) => id,
                            None => continue,
                        },
                    };
                    let Some(e) = state.entities.get(&a.uid) else {
                        continue;
                    };
                    let width = number(e, "width").max(1) as u32;
                    let height = number(e, "height").max(1) as u32;
                    // Archive dimensions are absent for some source entities;
                    // use the immutable source catalog rather than a guessed UI size.
                    let d = scene
                        .world
                        .as_ref()
                        .and_then(|w| w.catalog.get(&(number(e, "type") as u32)))
                        .map_or([width, height], |d| {
                            [d.width.max(1.) as u32, d.height.max(1.) as u32]
                        });
                    let mut d = d;
                    if [crate::world::SW, crate::world::NE]
                        .contains(&(number(e, "direction") as u32))
                    {
                        d.swap(0, 1)
                    }
                    let fan = production_fan(*count, d[0], d[1], random(), random());
                    for (instance, piece) in fan.into_iter().enumerate() {
                        self.effects.push(Effect {
                            mission: None,
                            lifetime: 3.75,
                            instance,
                            drop_sound_pending: true,
                            position: production_position(
                                position(state, &a.uid),
                                d,
                                random(),
                                random(),
                            ),
                            recipe: id,
                            sequence: a.sequence,
                            count: piece.count,
                            cosmetic: true,
                            offset: scatter(),
                            at: seconds + piece.delay,
                            fly: None,
                            path: None,
                        });
                    }
                }
            } else if matches!(
                a.kind.as_str(),
                "flight_upgrade"
                    | "flight_upgrade_touch"
                    | "flight_research"
                    | "flight_research_touch"
            ) {
                if let Some(e) = self.effects.iter_mut().find(|e| e.recipe == a.recipe) {
                    e.fly = Some(seconds);
                } else {
                    self.effects.push(Effect {
                        mission: None,
                        lifetime: 3.75,
                        instance: 0,
                        drop_sound_pending: false,
                        position: position(state, &a.uid),
                        recipe: a.recipe,
                        sequence: a.sequence,
                        count: 1,
                        cosmetic: false,
                        offset: scatter(),
                        at: seconds - 3.75,
                        fly: Some(seconds),
                        path: None,
                    });
                }
            }
        }
        self.revision = Some(state.revision);
        let mut refreshed = std::collections::BTreeSet::new();
        for e in &mut self.effects {
            if e.drop_sound_pending && seconds >= e.at {
                e.drop_sound_pending = false;
                self.sounds.push("ui/vipadanie_bonusov");
            }
            if e.cosmetic && e.fly.is_none() && seconds >= e.at + e.lifetime {
                e.fly = Some(e.at + e.lifetime);
                self.sounds.push(gather_sound(e.recipe));
                refreshed.insert(e.recipe);
            }
        }
        for id in refreshed {
            self.refresh_counter(id, seconds);
        }
        self.effects
            .retain(|e| e.fly.is_none_or(|t| seconds - t < 8.) && seconds - e.at < 30.);
    }
    fn refresh_counter(&mut self, id: u32, seconds: f64) {
        // addFallingItem changes the source profile per spawned piece. A very
        // early manual flight must not expose pieces still awaiting creation.
        let future = self
            .effects
            .iter()
            .filter(|e| e.cosmetic && e.recipe == id && e.at > seconds)
            .map(|e| e.count)
            .sum::<i64>();
        if future == 0 {
            self.held.remove(&id);
        } else {
            self.held.insert(id, future);
        }
    }
    pub fn hud_values(&self, scene: &Scene, state: &State, wall: i64) -> crate::hud::Values {
        let mut values = state.hud_values(scene, wall);
        for (id, key) in [(1, "playMoney"), (2, "exp"), (4, "fuel"), (5, "iron")] {
            if let Some(held) = self.held.get(&id) {
                let visible = state
                    .resources
                    .get(key)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(*held)
                    .max(0);
                values.texts.insert(key.into(), visible.to_string());
            }
        }
        values
    }
    pub fn take_sounds(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.sounds)
    }
    /// Capture the local path once; SceneMain keeps moving the world parent.
    pub fn prepare_camera(&mut self, scene: &Scene, camera: &Camera) {
        let scale = crate::hud::scale(camera);
        for e in &mut self.effects {
            if let Some(fly) = e.fly.filter(|_| e.path.is_none()) {
                let root = camera.screen(scene.projection.project(e.position[0], e.position[1]));
                let rise = drop_pose((fly - e.at).max(0.)).0;
                e.path = Some(FlightPath::new(
                    [root[0] / scale, root[1] / scale],
                    [e.offset[0], e.offset[1] + rise],
                    target(e.recipe),
                ));
            }
        }
    }
    pub fn pickup(&mut self, sequence: u64, id: u32, instance: usize, seconds: f64) {
        if let Some(e) = self.effects.iter_mut().find(|e| {
            e.cosmetic
                && e.sequence == sequence
                && e.recipe == id
                && e.instance == instance
                && seconds >= e.at
                && e.fly.is_none()
        }) {
            e.fly = Some(seconds.max(e.at));
            self.sounds.push(gather_sound(id));
            self.refresh_counter(id, seconds);
        }
    }
    pub fn draw(&self, v: &mut View, scene: &Scene, state: &State, camera: &Camera, seconds: f64) {
        let Some(c) = &scene.interface else {
            return;
        };
        let scale = crate::hud::scale(camera);
        for e in &self.effects {
            let mission = state
                .mission
                .as_ref()
                .filter(|_| !state.mission_paused())
                .map(|m| m.id.as_str());
            if e.mission.as_deref() != mission || seconds < e.at {
                continue;
            }
            let Some(icon) = (if e.cosmetic {
                c.symbols.get(&format!(
                    "falling_{}_{}",
                    if e.recipe < 100 { "resource" } else { "item" },
                    e.recipe
                ))
            } else {
                c.work_icons.get(&e.recipe)
            }) else {
                continue;
            };
            let point = camera.screen(scene.projection.project(e.position[0], e.position[1]));
            let elapsed = (seconds - e.at).max(0.);
            let (rise, grow, rotation) = drop_pose(e.fly.map_or(elapsed, |t| (t - e.at).max(0.)));
            let mut x = point[0] / scale + e.offset[0];
            let mut y = point[1] / scale + e.offset[1] + rise;
            let mut arrived = false;
            if let Some(fly) = e.fly {
                let path = e.path.unwrap_or_else(|| {
                    FlightPath::new(
                        [point[0] / scale, point[1] / scale],
                        [e.offset[0], e.offset[1] + rise],
                        target(e.recipe),
                    )
                });
                let pose = path.sample(seconds - fly);
                arrived = pose.2;
                x = point[0] / scale + pose.0;
                y = point[1] / scale + pose.1;
            }
            let factor = if e.recipe < 100 {
                1.
            } else {
                40. / icon.size[0]
            } * grow;
            let start = v.draws.len();
            if !arrived {
                v.add(
                    icon,
                    [x / factor, y / factor],
                    scale * factor,
                    |_| true,
                    |_| None,
                    None,
                );
                for d in &mut v.draws[start..] {
                    d.node.id = format!("bonus:{}:{}", e.recipe, d.node.id);
                    rotate_child(&mut d.node, [x * scale, y * scale], rotation);
                }
            }
            if e.fly.is_none() {
                v.hits.push(crate::interface::Hit {
                    action: if e.cosmetic {
                        crate::interface::Action::FallingPickup(e.sequence, e.recipe, e.instance)
                    } else {
                        crate::interface::Action::BonusPickup(e.recipe)
                    },
                    rect: [
                        (x - 20. * grow) * scale,
                        (y - 20. * grow) * scale,
                        40. * grow * scale,
                        40. * grow * scale,
                    ],
                });
            }
            // Count field remains the original falling_items template when available.
            if let Some(fly) = e.fly {
                if let Some(text) = c.symbols.get("falling_count") {
                    let p = ((seconds - fly) / 4.).clamp(0., 1.);
                    let at = v.draws.len();
                    v.add(
                        text,
                        // Separate spriteText child: initialized before drop
                        // scatter at y=-15, then moves up50 over four seconds.
                        [point[0] / scale, point[1] / scale - 15. - 50. * p],
                        scale,
                        |_| true,
                        |n| (n.id == "count_text").then(|| format!("+{}", e.count)),
                        None,
                    );
                    for d in &mut v.draws[at..] {
                        d.node.id = format!("bonus:{}:count", e.recipe);
                        d.node.parents.push("source_falling_count".into());
                        // Paired ARM phone FBO shows24px rendered counts.
                        // MTLabel fits its requested48px face to the original
                        //100x32frame. Retain that exported frame/fit here; growing
                        //both face and frame made the native count twice as big.
                        d.opacity = ((1. - p) * 255.) as u8;
                    }
                }
            }
        }
    }
}
/// FallingItem.animateDropWithLifetimeMode phase targets. Source rotation is in
/// radians; SDL uses degrees. SPTransitions easeIn/out are cubic (0x21a254/268).
pub fn drop_pose(t: f64) -> (f64, f64, f64) {
    let lerp = |a: f64, b: f64, p: f64| a + (b - a) * p.clamp(0., 1.);
    if t < 0.05 {
        (0., lerp(1., 0.05, t / 0.05), lerp(0., -20., t / 0.05))
    } else if t < 0.35 {
        let p = ease_out((t - 0.05) / 0.3);
        (lerp(0., -50., p), lerp(0.05, 1., p), lerp(-20., 20., p))
    } else if t < 0.55 {
        let p = ease_in((t - 0.35) / 0.2);
        (lerp(-50., 0., p), 1., lerp(20., 35., p))
    } else if t < 0.65 {
        let p = ease_out((t - 0.55) / 0.1);
        (lerp(0., -10., p), 1., lerp(35., 18., p))
    } else if t < 0.75 {
        let p = ease_in((t - 0.65) / 0.1);
        (lerp(-10., 0., p), 1., lerp(18., 0., p))
    } else {
        (0., 1., 0.)
    }
}

/// Source spriteAnimation coordinates, relative to its moving world parent.
/// Thumb0x11e536 selects left/right control points;0x11e6d0 passes s22/s20.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightPath {
    pub start: [f64; 2],
    pub focus: [f64; 2],
    pub target: [f64; 2],
    pub duration: f64,
}
impl FlightPath {
    pub fn new(root: [f64; 2], start: [f64; 2], hud_target: [f64; 2]) -> Self {
        let target = [hud_target[0] - root[0], hud_target[1] - root[1]];
        let focus = if start[0] < target[0] {
            [-50., -30.]
        } else {
            [target[0] - 50., -50.]
        };
        let duration = ((target[0] - focus[0]).hypot(target[1] - focus[1])
            + (start[0] - focus[0]).hypot(start[1] - focus[1]))
            * 0.33
            / 200.;
        Self {
            start,
            focus,
            target,
            duration,
        }
    }
    pub fn sample(self, elapsed: f64) -> (f64, f64, bool) {
        let p = (elapsed / self.duration.max(f64::EPSILON)).clamp(0., 1.);
        let a = 1. - p;
        (
            a * a * self.start[0] + 2. * a * p * self.focus[0] + p * p * self.target[0],
            a * a * self.start[1] + 2. * a * p * self.focus[1] + p * p * self.target[1],
            p >= 1.,
        )
    }
}
/// Origin-free helper: root is the initial icon position, without scatter.
pub fn flight_pose(start: [f64; 2], elapsed: f64) -> (f64, f64, bool) {
    let p = FlightPath::new(start, [0., 0.], [0., 15.]).sample(elapsed);
    (start[0] + p.0, start[1] + p.1, p.2)
}

fn rotate_child(node: &mut crate::interface::Node, origin: [f64; 2], degrees: f64) {
    // SDL rotates a texture about its center; SP rotates the complete parent
    // about(0,0). Rotate child centers together, retaining authored rotations.
    let a = degrees.to_radians();
    let (c, s) = (a.cos(), a.sin());
    let center = [
        node.rect[0] + node.rect[2] / 2. - origin[0],
        node.rect[1] + node.rect[3] / 2. - origin[1],
    ];
    node.rect[0] = origin[0] + c * center[0] - s * center[1] - node.rect[2] / 2.;
    node.rect[1] = origin[1] + s * center[0] + c * center[1] - node.rect[3] / 2.;
    node.rotation += degrees;
}

fn ease_in(t: f64) -> f64 {
    t.clamp(0., 1.).powi(3)
}
fn ease_out(t: f64) -> f64 {
    1. + (t.clamp(0., 1.) - 1.).powi(3)
}
fn scatter() -> [f64; 2] {
    // Separate OS entropy, never the saved production/combat stream. Failure
    // suppresses scatter rather than preventing an already committed reward.
    let axis = || crate::save::entropy().map_or(0., |w| (w % 31) as f64);
    [axis(), axis()]
}
fn target(id: u32) -> [f64; 2] {
    // CHUDLayer icon centers in the original480point phone top_hud.
    [
        match id {
            1 => 275.,
            2 => 15.825,
            4 => 368.325,
            5 => 432.6,
            _ => 0.,
        },
        15.,
    ]
}
fn gather_sound(id: u32) -> &'static str {
    match id {
        1 => "ui/coin_take",
        2 => "ui/get_exp",
        4 => "ui/oil_take",
        5 => "ui/ore_take",
        _ => "ui/get_item",
    }
}

fn position(state: &State, uid: &str) -> [f64; 2] {
    let e = &state.entities[uid];
    [(number(e, "x") - 16) as f64, (number(e, "y") - 16) as f64]
}

/// Source0x89961: area/5 is INTEGER before roundf; choose n in0..=area/5.
/// Divide by n+1, round an inexact quotient to its leading decimal digit,
/// and emit the exact remainder last. Counts1..4 remain a single bonus.
/// These are visual denominations only: actual gains/entropy stay in the journal.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub count: i64,
    pub delay: f64,
}
pub fn production_fan(
    count: i64,
    width: u32,
    height: u32,
    word: u32,
    delay_word: u32,
) -> Vec<Piece> {
    if count <= 0 {
        return vec![];
    }
    let n = word % ((width.clamp(1, 128) * height.clamp(1, 128)) / 5 + 1);
    if count <= 4 {
        return vec![Piece {
            count,
            delay: (delay_word % (n + 2)) as f64 * 0.1,
        }];
    }
    let divisor = n as i64 + 1;
    let mut chunk = count / divisor;
    if count % divisor != 0 && chunk > 0 {
        let mut place = 1;
        while chunk / place >= 10 {
            place *= 10;
        }
        chunk -= chunk % place;
    }
    let mut left = count;
    let mut out = vec![];
    for i in 0..=n + 1 {
        let amount = if i == n + 1 { left } else { chunk.min(left) };
        // Actual source may create zero-count sprites for tiny-count/large
        // footprints; retain their timing while never minting a reward.
        out.push(Piece {
            count: amount,
            delay: i as f64 * 0.1,
        });
        left -= amount;
        if left == 0 {
            break;
        }
    }
    out
}
fn random() -> u32 {
    crate::save::entropy().unwrap_or(0)
}

/// SceneMain's CGRect spans the inclusive real footprint. radiusForSquare
///0x7ab29=w*sqrt(2)/2; radiusForRectangle0x7aaf1=sqrt((w*w+h*h)/2)
///after INTEGERconversion/division. addFallingItem0x7ab59 adds0.3 for1x1,
///uses separate positive rand()/RAND_MAX axes, then truncates tile coordinates.
pub fn production_position(center: [f64; 2], dimensions: [u32; 2], rx: u32, ry: u32) -> [f64; 2] {
    let w = dimensions[0].clamp(1, 128) as i32;
    let h = dimensions[1].clamp(1, 128) as i32;
    let rw = w - (1 - w % 2);
    let rh = h - (1 - h % 2);
    let radius = if rw == rh {
        rw as f64 * std::f64::consts::SQRT_2 * 0.5 + if rw == 1 { 0.3 } else { 0. }
    } else {
        (((rw * rw + rh * rh) / 2) as f64).sqrt()
    };
    let ratio = |word: u32| (word & 0x7fff_ffff) as f64 / i32::MAX as f64;
    [
        (center[0] - (w / 2) as f64 + radius * ratio(rx)).trunc(),
        (center[1] - (h / 2) as f64 + radius * ratio(ry)).trunc(),
    ]
}
