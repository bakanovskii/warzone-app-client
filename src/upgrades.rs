//! Original Upgrades/UpgradeItem graph, preferences and academy transactions.
use crate::{
    Scene,
    pricing::{Quote, special_price},
    production::{Command, State, adjust, number},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Modifier {
    pub kind: String,
    pub ids: Vec<u32>,
    pub values: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Upgrade {
    pub id: u32,
    pub name: String,
    pub seconds: i64,
    pub unlock_price: i64,
    pub parents: Vec<u32>,
    pub research: Vec<u32>,
    pub cost: BTreeMap<String, i64>,
    pub items: BTreeMap<u32, i64>,
    pub modifiers: Vec<Modifier>,
    pub icon: String,
}
impl Upgrade {
    pub fn affects(&self, unit: u32) -> bool {
        self.modifiers.iter().any(|m| m.ids.contains(&unit))
    }
    pub fn model(&self, unit: u32) -> Option<(&str, i64)> {
        self.modifiers.iter().find_map(|m| {
            (m.kind == "model" && m.ids.contains(&unit)).then(|| {
                Some((
                    m.values.get("model")?.as_str()?,
                    m.values.get("viewlevel")?.as_i64()?,
                ))
            })?
        })
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub items: BTreeMap<u32, Upgrade>,
    pub unit_order: Vec<u32>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelArt>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelArt {
    pub unit: u32,
    pub level: i64,
    pub thumbnail: String,
    pub variants: BTreeMap<u32, Vec<crate::Track>>,
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        if self.items.is_empty() || self.items.len() > 1000 || self.unit_order.len() > 32 {
            return Err("Invalid original upgrade catalog".into());
        }
        fn visit(
            c: &Config,
            id: u32,
            active: &mut BTreeSet<u32>,
            done: &mut BTreeSet<u32>,
        ) -> Result<(), String> {
            if done.contains(&id) {
                return Ok(());
            }
            if !active.insert(id) {
                return Err("Upgrade dependency cycle".into());
            }
            let r = c.items.get(&id).ok_or("Unknown upgrade parent")?;
            for p in &r.parents {
                visit(c, *p, active, done)?;
            }
            active.remove(&id);
            done.insert(id);
            Ok(())
        }
        let mut done = BTreeSet::new();
        for (id, r) in &self.items {
            if *id != r.id
                || !(11000..12000).contains(id)
                || !(1..864000).contains(&r.seconds)
                || !(0..i32::MAX as i64).contains(&r.unlock_price)
                || !scene.textures.contains_key(&r.icon)
                || r.modifiers.is_empty()
                || r.items
                    .iter()
                    .any(|(i, n)| !(4000..5000).contains(i) || *n <= 0)
                || r.cost.iter().any(|(k, n)| {
                    !["playMoney", "fuel", "iron", "realMoney"].contains(&k.as_str()) || *n < 0
                })
            {
                return Err("Invalid source upgrade".into());
            }
            for m in &r.modifiers {
                if m.ids.is_empty()
                    || m.values.is_empty()
                    || !["item", "recipe", "model"].contains(&m.kind.as_str())
                {
                    return Err("Invalid upgrade modifier".into());
                }
                for (key, v) in &m.values {
                    let valid = match (m.kind.as_str(), key.as_str()) {
                        ("item", "health" | "attack" | "speed" | "distance" | "population") => {
                            v.as_i64().is_some_and(|n| (-100000..100000).contains(&n))
                        }
                        ("recipe", "time") => {
                            v.as_i64().is_some_and(|n| (-864000..864000).contains(&n))
                        }
                        ("recipe", "incoming") => v
                            .as_f64()
                            .is_some_and(|n| n.is_finite() && (-1. ..=1.).contains(&n)),
                        ("model", "viewlevel") => v.as_i64().is_some_and(|n| (1..=10).contains(&n)),
                        ("model", "model") => v.as_str().is_some_and(|s| {
                            !s.is_empty()
                                && s.len() < 128
                                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        }),
                        _ => false,
                    };
                    if !valid {
                        return Err("Unsupported source upgrade modifier".into());
                    }
                }
            }
            visit(self, *id, &mut BTreeSet::new(), &mut done)?;
        }
        for unit in &self.unit_order {
            if self.layers(*unit)?.is_empty() {
                return Err("Empty source unit upgrade tree".into());
            }
        }
        if !self.models.is_empty() {
            let expected: BTreeMap<_, _> = self
                .items
                .values()
                .flat_map(|u| {
                    self.unit_order
                        .iter()
                        .filter_map(|id| u.model(*id).map(|(name, level)| (name, (*id, level))))
                })
                .collect();
            if expected.len() != self.models.len() {
                return Err("Incomplete original upgrade models".into());
            }
            for (name, m) in &self.models {
                if expected.get(name.as_str()) != Some(&(m.unit, m.level))
                    || !scene.textures.contains_key(&m.thumbnail)
                    || m.variants.len() != 8
                {
                    return Err("Invalid original upgrade model".into());
                }
                for (direction, tracks) in &m.variants {
                    if ![
                        0x100000, 0x200000, 0x400000, 0x800000, 0x1000000, 0x2000000, 0x4000000,
                        0x8000000,
                    ]
                    .contains(direction)
                        || tracks.is_empty()
                    {
                        return Err("Missing upgrade model direction".into());
                    }
                    for t in tracks {
                        let texture = scene
                            .textures
                            .get(&t.texture)
                            .ok_or("Missing upgrade atlas")?;
                        if !t.fps.is_finite()
                            || !(0.0..=60.).contains(&t.fps)
                            || t.frames.is_empty()
                            || t.frames.len() > 256
                        {
                            return Err("Invalid upgrade animation".into());
                        }
                        for p in t.frames.iter().flatten() {
                            if p.src[2] == 0
                                || p.src[3] == 0
                                || p.src[0]
                                    .checked_add(p.src[2])
                                    .is_none_or(|n| n > texture.width)
                                || p.src[1]
                                    .checked_add(p.src[3])
                                    .is_none_or(|n| n > texture.height)
                                || !p.rect.iter().all(|n| n.is_finite())
                                || p.rect[2] <= 0.
                                || p.rect[3] <= 0.
                            {
                                return Err("Upgrade sprite outside source atlas".into());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// Upgrades.getUpgradesForUnit sorts IDs and moves descendants to later
    /// columns until no dependency remains in a column with its parent.
    pub fn layers(&self, unit: u32) -> Result<Vec<Vec<u32>>, String> {
        let mut pending: BTreeSet<_> = self
            .items
            .values()
            .filter(|u| u.affects(unit))
            .map(|u| u.id)
            .collect();
        let mut out = Vec::new();
        while !pending.is_empty() {
            let layer: Vec<_> = pending
                .iter()
                .copied()
                .filter(|id| self.items[id].parents.iter().all(|p| !pending.contains(p)))
                .collect();
            if layer.is_empty() {
                return Err("Cyclic unit upgrade tree".into());
            }
            for id in &layer {
                pending.remove(id);
            }
            out.push(layer);
        }
        Ok(out)
    }
}
impl State {
    /// Appearance is derived from preferences, never written into source entities
    /// or serialized battle troops. Old checksummed saves keep their exact shape.
    pub fn unit_thumbnail<'a>(&self, scene: &'a Scene, unit: u32) -> Option<&'a str> {
        self.upgraded_model(scene, unit)
            .and_then(|name| scene.upgrades.as_ref()?.models.get(name))
            .map(|m| m.thumbnail.as_str())
            .or_else(|| {
                scene
                    .interface
                    .as_ref()?
                    .unit_thumbs
                    .get(&unit)
                    .map(String::as_str)
            })
    }
    pub fn upgraded_tracks<'a>(
        &self,
        scene: &'a Scene,
        unit: u32,
        direction: u32,
    ) -> Option<&'a Vec<crate::Track>> {
        scene
            .upgrades
            .as_ref()?
            .models
            .get(self.upgraded_model(scene, unit)?)?
            .variants
            .get(&direction)
    }
    pub fn mission_scene(&self, scene: &Scene) -> Result<Scene, String> {
        let models = scene
            .upgrades
            .as_ref()
            .into_iter()
            .flat_map(|c| &c.unit_order)
            .filter_map(|id| {
                self.upgraded_model(scene, *id)
                    .filter(|name| {
                        scene
                            .upgrades
                            .as_ref()
                            .is_some_and(|c| c.models.contains_key(*name))
                    })
                    .map(|m| (*id, m.to_string()))
            })
            .collect();
        self.mission
            .as_ref()
            .ok_or("Missing mission")?
            .battle
            .render_scene_with_models(scene, &models)
    }
    pub fn upgraded_player_definitions(
        &self,
        scene: &Scene,
    ) -> BTreeMap<u32, crate::battle::Definition> {
        scene
            .combat
            .as_ref()
            .into_iter()
            .flat_map(|c| &c.defs)
            .filter(|(_, d)| d.group == "units")
            .map(|(id, d)| {
                let mut d = d.clone();
                d.health = self.upgraded_stat(scene, *id, "health", d.health as i64) as i32;
                d.attack = self.upgraded_stat(scene, *id, "attack", d.attack as i64) as i32;
                d.distance = self.upgraded_stat(scene, *id, "distance", d.distance as i64) as i32;
                d.speed = self.upgraded_stat(scene, *id, "speed", d.speed as i64) as i32;
                (*id, d)
            })
            .collect()
    }
    /// Historical recipe prices/timers must survive later completed upgrades.
    pub fn upgrade_ready_before(&self, id: u32, sequence: u64) -> bool {
        self.upgrade_ready(id)
            && !self.actions.iter().any(|a| {
                a.recipe == id
                    && a.sequence >= sequence
                    && matches!(a.kind.as_str(), "upgrade_finish" | "upgrade_buy")
            })
    }
    pub fn upgraded_stat(&self, scene: &Scene, unit: u32, key: &str, base: i64) -> i64 {
        // Reject unrelated catalog modifiers before looking up completion.
        // The old order formatted/queried every Upgrade-ID for every obstacle,
        // building and repeated unit when drawing an ordinary shop/HUD frame.
        base + scene
            .upgrades
            .as_ref()
            .into_iter()
            .flat_map(|c| c.items.values())
            .flat_map(|u| {
                u.modifiers.iter().filter_map(move |m| {
                    if m.kind != "item" || !m.ids.contains(&unit) {
                        return None;
                    }
                    m.values
                        .get(key)
                        .and_then(Value::as_i64)
                        .filter(|_| self.upgrade_ready(u.id))
                })
            })
            .sum::<i64>()
    }
    pub fn upgraded_model<'a>(&self, scene: &'a Scene, unit: u32) -> Option<&'a str> {
        scene
            .upgrades
            .as_ref()?
            .items
            .values()
            .filter_map(|u| u.model(unit).filter(|_| self.upgrade_ready(u.id)))
            .max_by_key(|(_, level)| *level)
            .map(|(model, _)| model)
    }
    pub fn upgraded_recipe(
        &self,
        scene: &Scene,
        r: &crate::workshop::Recipe,
        sequence: u64,
    ) -> crate::workshop::Recipe {
        let mut r = r.clone();
        for u in scene
            .upgrades
            .as_ref()
            .into_iter()
            .flat_map(|c| c.items.values())
            .filter(|u| {
                u.modifiers
                    .iter()
                    .any(|m| m.kind == "recipe" && m.ids.contains(&r.id))
            })
            .filter(|u| self.upgrade_ready_before(u.id, sequence))
        {
            for m in u
                .modifiers
                .iter()
                .filter(|m| m.kind == "recipe" && m.ids.contains(&r.id))
            {
                if let Some(n) = m.values.get("time").and_then(Value::as_i64) {
                    r.seconds += n;
                }
                if let Some(n) = m.values.get("incoming").and_then(Value::as_f64) {
                    // Upgrades.applyUpgradesToRecipe uses VFP binary32, then
                    // signed truncation, separately for every currency.
                    let factor = 1.0f32 + n as f32;
                    for amount in r.cost.values_mut() {
                        *amount = (*amount as f32 * factor) as i64;
                    }
                }
            }
        }
        r
    }
    pub(crate) fn workshop_origin_sequence(
        &self,
        uid: &str,
        id: u32,
        started: i64,
        before: u64,
    ) -> u64 {
        let origin = self.production_origin(uid, id, started, before);
        self.actions
            .iter()
            .find(|a| {
                a.uid == uid
                    && a.recipe == id
                    && a.at == origin
                    && a.kind == "workshop_start"
                    && a.sequence < before
            })
            .map_or(0, |a| a.sequence)
    }
    pub fn upgrade_ready(&self, id: u32) -> bool {
        self.systems
            .as_ref()
            .is_some_and(|s| s.flag(&format!("Upgrade-{id}")))
    }
    pub fn upgrade_unlocked(&self, u: &Upgrade) -> bool {
        let key = format!("UpgradeUnlock-{}", u.id);
        self.systems.as_ref().is_some_and(|s| {
            if s.preferences.contains_key(&key) {
                s.flag(&key)
            } else {
                u.parents.iter().all(|p| self.upgrade_ready(*p))
            }
        })
    }
    pub fn upgrade_command(&self, uid: &str, id: u32) -> Option<Command> {
        if self.upgrade_gathering(uid) {
            return None;
        }
        let e = self.entities.get(uid)?;
        if number(e, "type") != 1019 {
            return None;
        }
        Some(if number(e, "productionType") == 0 {
            Command::Start {
                uid: uid.into(),
                recipe: id,
            }
        } else {
            Command::Collect {
                uid: uid.into(),
                recipe: number(e, "productionType") as u32,
                started: number(e, "productionStartTime"),
            }
        })
    }
    fn academy<'a>(
        &self,
        scene: &'a Scene,
        uid: &str,
        id: u32,
        wall: i64,
        background: bool,
    ) -> Result<(&'a Upgrade, i64), String> {
        if self.mission.is_some() && !background {
            return Err("Сначала вернись на базу".into());
        }
        let e = self.entities.get(uid).ok_or("Академия не найдена")?;
        if self.upgrade_gathering(uid) {
            return Err("Улучшение уже получается".into());
        }
        if number(e, "type") != 1019
            || number(e, "state") != 3
            || e["isDeleted"] == true
            || e["isInBattle"] == true
        {
            return Err("Академия сейчас недоступна".into());
        }
        let u = scene
            .upgrades
            .as_ref()
            .and_then(|c| c.items.get(&id))
            .ok_or("Улучшение не найдено")?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time
            || now
                .checked_add(u.seconds)
                .is_none_or(|t| t >= i32::MAX as i64)
        {
            return Err("Недопустимое время улучшения".into());
        }
        Ok((u, now))
    }
    pub fn upgrade_quote(&self, scene: &Scene, u: &Upgrade) -> Result<Quote, String> {
        let c = scene.pricing.as_ref().ok_or("Missing upgrade prices")?;
        let mut cash = c.time(u.seconds)?;
        let mut consume = BTreeMap::new();
        for (k, need) in &u.cost {
            let used = (*need).min(self.resources.get(k).copied().unwrap_or(0));
            cash = cash
                .checked_add(if k == "realMoney" {
                    need - used
                } else {
                    c.resource(k, need - used)?
                })
                .ok_or("Upgrade price overflow")?;
            consume.insert(k.clone(), used);
        }
        for (id, need) in &u.items {
            let used = (*need).min(self.inventory.get(&id.to_string()).copied().unwrap_or(0));
            cash = cash
                .checked_add(
                    special_price(scene, *id)?
                        .checked_mul(need - used)
                        .ok_or("Upgrade price overflow")?,
                )
                .ok_or("Upgrade price overflow")?;
            consume.insert(format!("item:{id}"), used);
        }
        Ok(Quote { cash, consume })
    }
    fn finish_upgrade_flags(&mut self, scene: &Scene, id: u32, quest: bool) -> Result<(), String> {
        let s = self.systems.as_mut().ok_or("Missing upgrade preferences")?;
        s.set(format!("Upgrade-{id}"), 1);
        s.set("lastUpgrade".into(), 0);

        s.achievement("complete_upgrades", 1, false);
        if let Some(c) = &scene.progression.as_ref().filter(|_| quest) {
            s.event(c, self.resources["level"], 23, id)?;
        }
        Ok(())
    }
    pub fn upgrade_apply(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
    ) -> Result<(Self, String), String> {
        self.apply_upgrade(scene, cmd, wall, false, false, None)
    }
    pub(crate) fn upgrade_apply_background(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        defer_quest: bool,
        at_ms: Option<i64>,
    ) -> Result<(Self, String), String> {
        self.apply_upgrade(scene, cmd, wall, true, defer_quest, at_ms)
    }
    fn apply_upgrade(
        &self,
        scene: &Scene,
        cmd: &Command,
        wall: i64,
        background: bool,
        defer_quest: bool,
        at_ms: Option<i64>,
    ) -> Result<(Self, String), String> {
        let (uid, id) = match cmd {
            Command::Start { uid, recipe } | Command::Collect { uid, recipe, .. } => (uid, *recipe),
        };
        let (u, now) = self.academy(scene, uid, id, wall, background)?;
        if self.upgrade_ready(id) {
            return Err("Улучшение уже выполнено".into());
        }
        let mut next = self.clone();
        let mut gains = BTreeMap::new();
        let kind = match cmd {
            Command::Start { .. } => {
                if !self.upgrade_unlocked(u) {
                    return Err("Сначала открой предыдущее улучшение".into());
                }
                if number(&self.entities[uid], "productionType") != 0
                    || self
                        .entities
                        .values()
                        .any(|v| v["isDeleted"] != true && number(v, "productionType") == id as i64)
                {
                    return Err("Улучшение уже идёт".into());
                }
                for (k, n) in &u.cost {
                    adjust(&mut next.resources, k, -n)?;
                    gains.insert(k.clone(), -n);
                }
                for (i, n) in &u.items {
                    adjust(&mut next.inventory, &i.to_string(), -n)?;
                    gains.insert(format!("item:{i}"), -n);
                }
                let e = next.entities.get_mut(uid).unwrap();
                e["productionType"] = id.into();
                e["productionStartTime"] = now.into();
                let s = next.systems.as_mut().ok_or("Missing upgrade state")?;
                s.set("lastUpgrade".into(), id as i64);
                s.set("sessionsTimeWithoutUpgrades".into(), 0);
                "upgrade_start"
            }
            Command::Collect { started, .. } => {
                let e = &self.entities[uid];
                if number(e, "productionType") != id as i64
                    || number(e, "productionStartTime") != *started
                    || started.checked_add(u.seconds).is_none_or(|end| now <= end)
                {
                    return Err("Улучшение ещё не готово или уже изменилось".into());
                }
                next.finish_upgrade_flags(scene, id, !defer_quest)?;
                let e = next.entities.get_mut(uid).unwrap();
                e["productionType"] = 0.into();
                e["productionStartTime"] = 0.into();
                "upgrade_finish"
            }
        };
        next.building_receipt(now, uid, id, kind, gains, vec![])?;
        next.actions.last_mut().unwrap().at_ms = at_ms;
        next.validate(scene)?;
        Ok((
            next,
            format!(
                "{}: {}",
                if kind == "upgrade_start" {
                    "Начато улучшение"
                } else {
                    "Улучшение выполнено"
                },
                u.name
            ),
        ))
    }
    pub fn buy_upgrade(
        &self,
        scene: &Scene,
        uid: &str,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let (u, now) = self.academy(scene, uid, id, wall, false)?;
        if self.upgrade_ready(id)
            || !self.upgrade_unlocked(u)
            || number(&self.entities[uid], "productionType") != 0
        {
            return Err("Покупка улучшения сейчас недоступна".into());
        }
        let q = self.upgrade_quote(scene, u)?;
        let mut next = self.clone();
        next.charge_cash(q.cash)?;
        let mut gains = BTreeMap::from([("realMoney".into(), -q.cash)]);
        for (k, n) in q.consume {
            if let Some(i) = k.strip_prefix("item:") {
                adjust(&mut next.inventory, i, -n)?;
            } else {
                adjust(&mut next.resources, &k, -n)?;
            }
            gains.insert(k, -n);
        }
        next.finish_upgrade_flags(scene, id, true)?;
        next.systems
            .as_mut()
            .unwrap()
            .set("sessionsTimeWithoutUpgrades".into(), 0);
        next.building_receipt(now, uid, id, "upgrade_buy", gains, vec![])?;
        next.validate(scene)?;
        Ok((next, format!("Улучшение выполнено: {}", u.name)))
    }
    pub(crate) fn validate_upgrades(&self, scene: &Scene) -> Result<(), String> {
        let Some(c) = &scene.upgrades else {
            return Ok(());
        };
        let mut starts = BTreeMap::new();
        let mut done = BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind.starts_with("upgrade_") && a.kind != "upgrade_speedup")
        {
            let u = c.items.get(&a.recipe).ok_or("Unknown upgrade receipt")?;
            if self
                .entities
                .get(&a.uid)
                .is_none_or(|e| number(e, "type") != 1019)
                || a.at <= 0
                || a.at > self.last_action_time
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
                || a.shop.is_some()
                || a.source.is_some()
            {
                return Err("Invalid upgrade receipt".into());
            }
            let costs: BTreeMap<String, i64> = u
                .cost
                .iter()
                .map(|(k, n)| (k.clone(), -n))
                .chain(u.items.iter().map(|(i, n)| (format!("item:{i}"), -n)))
                .collect();
            match a.kind.as_str() {
                "upgrade_start" => {
                    if a.gains != costs
                        || starts.insert(a.recipe, a).is_some()
                        || done.contains(&a.recipe)
                    {
                        return Err("Invalid/duplicate upgrade start".into());
                    }
                }
                "upgrade_finish" => {
                    let source = starts.get(&a.recipe).map(|b| (&b.uid, b.at, b.sequence));
                    let imported = scene
                        .entities
                        .iter()
                        .find(|e| {
                            e.uid == a.uid
                                && number(&e.archived, "productionType") == a.recipe as i64
                        })
                        .map(|e| (&e.uid, number(&e.archived, "productionStartTime"), 0));
                    let (uid, started, seq) = source
                        .or(imported)
                        .ok_or("Upgrade completion without origin")?;
                    let forced = self
                        .actions
                        .iter()
                        .find(|f| {
                            f.kind == "upgrade_speedup"
                                && f.uid == a.uid
                                && f.recipe == a.recipe
                                && f.sequence > seq
                                && f.sequence < a.sequence
                        })
                        .map_or(started, |f| f.at - 864000);
                    if uid != &a.uid
                        || a.at <= forced + u.seconds
                        || !a.gains.is_empty()
                        || !done.insert(a.recipe)
                        || !self.upgrade_ready(a.recipe)
                    {
                        return Err("Invalid upgrade completion".into());
                    }
                }
                "upgrade_buy" => {
                    let mut cash = scene
                        .pricing
                        .as_ref()
                        .ok_or("Missing upgrade pricing")?
                        .time(u.seconds)?;
                    if a.gains.len() != 1 + costs.len() {
                        return Err("Invalid upgrade purchase inputs".into());
                    }
                    for (k, neg) in &costs {
                        let used = a
                            .gains
                            .get(k)
                            .copied()
                            .and_then(i64::checked_neg)
                            .filter(|n| *n >= 0 && *n <= -*neg)
                            .ok_or("Invalid upgrade input receipt")?;
                        let missing = -*neg - used;
                        cash = cash
                            .checked_add(if let Some(i) = k.strip_prefix("item:") {
                                special_price(
                                    scene,
                                    i.parse().map_err(|_| "Invalid upgrade item")?,
                                )? * missing
                            } else if k == "realMoney" {
                                missing
                            } else {
                                scene.pricing.as_ref().unwrap().resource(k, missing)?
                            })
                            .ok_or("Upgrade price overflow")?;
                    }
                    if a.gains.get("realMoney") != Some(&-cash)
                        || !done.insert(a.recipe)
                        || !self.upgrade_ready(a.recipe)
                        || starts.contains_key(&a.recipe)
                    {
                        return Err("Invalid/duplicate upgrade purchase".into());
                    }
                }
                _ => return Err("Unknown upgrade action".into()),
            }
        }
        for (uid, e) in &self.entities {
            if number(e, "type") == 1019 && number(e, "productionType") != 0 {
                let id = number(e, "productionType") as u32;
                let imported = scene
                    .entities
                    .iter()
                    .find(|s| s.uid == *uid && s.archived["productionType"] == id)
                    .map(|s| number(&s.archived, "productionStartTime"));
                let start = starts
                    .get(&id)
                    .filter(|s| s.uid == *uid)
                    .map(|s| s.at)
                    .or(imported);
                if !c.items.contains_key(&id)
                    || self.upgrade_ready(id)
                    || start.is_none_or(|start| {
                        self.production_forced_start(uid, id, start, u64::MAX)
                            != number(e, "productionStartTime")
                    })
                {
                    return Err("Invalid live academy timer".into());
                }
            }
        }
        Ok(())
    }
}
