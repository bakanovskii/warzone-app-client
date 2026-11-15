//! Atomic profile integration for native mission sessions. Candidate-only changes:
//! caller commits through Profile before publishing a move, casualty or reward.
use crate::{
    Scene,
    battle::{ArmyUnit, Battle},
    production::{ProductionSource, State, number},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub camera_serial: u64,
    pub camera: [f64; 3],
    pub id: String,
    pub opened: i64,
    pub reward_eligible: bool,
    pub battle: Battle,
}
fn is_zero(n: &u64) -> bool {
    *n == 0
}
#[derive(Clone, Debug)]
pub enum Command {
    RefillEnergy,
    PvpDialogue,
    FinishPvp,
    PvpTimeout,
    Open(String),
    Suspend,
    Resume(String),
    AdvanceDialogue,
    Tutorial(crate::battle::TutorialCommand),
    Place([i32; 2], u32),
    Unplace(String),
    Reposition(String, [i32; 2]),
    Start,
    Move(String, [i32; 2]),
    Attack(String, String),
    AttackFrom(String, String, [i32; 2]),
    AttackAll(String),
    Skip,
    CancelPlacement,
    Surrender,
    Retreat,
    Return,
}
impl State {
    /// Native scene visibility uses an existing preference-map entry, so old
    /// checksummed saves acquire no implicit serialized field on deserialization.
    pub fn mission_paused(&self) -> bool {
        self.systems
            .as_ref()
            .is_some_and(|p| p.flag("__native_mission_paused"))
    }
    pub fn mission_access(&self, scene: &Scene, name: &str) -> Result<(), String> {
        let c = scene.combat.as_ref().ok_or("Нет данных миссий")?;
        let m = c.missions.get(name).ok_or("Неизвестная миссия")?;
        let p = self.systems.as_ref().ok_or("Нет игрового профиля")?;
        if m.mission_id == 5990 {
            if c.pvp.is_none() {
                return Err("Нет данных PvP-обучения".into());
            }
            if p.flag("TutorialPVPComplete") {
                return Err(
                    "PvP-обучение пройдено. Сервер поиска противника пока не подключён.".into(),
                );
            }
            if self.resources["level"] < 5 {
                return Err("Сражения доступны с 5 уровня".into());
            }
            if self.mission_roster(scene).is_empty() {
                return Err("Сначала подготовь армию".into());
            }
            if self.resources["playMoney"] < crate::pvp::match_price(self.resources["level"])? {
                return Err("Недостаточно монет для поиска противника".into());
            }
        }
        if m.parent != 0
            && !p.flag(&format!("Mission-{}", m.parent))
            && !p.flag(&format!("MissionUnlock-{}", m.mission_id))
        {
            let title = c
                .missions
                .values()
                .find(|p| p.mission_id == m.parent)
                .map_or_else(|| m.parent.to_string(), |p| p.title.clone());
            return Err(format!("Сначала пройди миссию «{title}»"));
        }
        Ok(())
    }
    pub fn mission_roster(&self, scene: &Scene) -> Vec<ArmyUnit> {
        let Some(c) = &scene.combat else {
            return vec![];
        };
        self.entities
            .iter()
            .filter_map(|(uid, v)| {
                let id = number(v, "type") as u32;
                (v["isDeleted"] != true
                    && v["isInBattle"] != true
                    && c.defs.get(&id).is_some_and(|d| d.group == "units"))
                .then(|| ArmyUnit {
                    uid: uid.clone(),
                    type_id: id,
                })
            })
            .collect()
    }
    pub fn unit_in_mission(&self, uid: &str) -> bool {
        self.mission.as_ref().is_some_and(|s| {
            s.battle.phase == "battle"
                && s.battle.result.is_none()
                && s.battle
                    .units
                    .iter()
                    .any(|e| e.profile.as_deref() == Some(uid) && e.hp > 0)
        })
    }
    fn mission_receipt(
        &mut self,
        now: i64,
        s: &Session,
        uid: &str,
        kind: &str,
        gains: BTreeMap<String, i64>,
    ) -> Result<(), String> {
        let id = number(
            self.entities.get(uid).ok_or("Missing mission troop")?,
            "type",
        ) as u32;
        self.building_receipt(now, uid, id, kind, gains, vec![])?;
        self.actions.last_mut().unwrap().source = Some(ProductionSource {
            uid: s.id.clone(),
            started: s.opened,
        });
        Ok(())
    }
    pub fn mission_apply(
        &self,
        scene: &Scene,
        command: &Command,
        wall: i64,
    ) -> Result<(Self, String), String> {
        self.mission_apply_with_entropy(scene, command, wall, || {
            Err("PvP allocation requires an entropy provider".into())
        })
    }
    pub fn mission_apply_with_entropy(
        &self,
        scene: &Scene,
        command: &Command,
        wall: i64,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(Self, String), String> {
        let c = scene
            .combat
            .as_ref()
            .ok_or("В этом пакете нет нативных миссий")?;
        if matches!(command, Command::Return) {
            if let Some(s) = self
                .mission
                .as_ref()
                .filter(|s| s.battle.tutorial.is_some())
            {
                use crate::battle::TutorialCommand as T;
                // BattleEndDialog's OK completes the battle part and returns
                // to SceneMain; ID9 is its base-quest focus, not a second
                // instruction to press the tactical return button.
                let dismissed = if s.battle.tutorial_id(c) == 8 {
                    self.mission_apply(scene, &Command::Tutorial(T::DismissResult), wall)?
                        .0
                } else {
                    self.clone()
                };
                return dismissed.mission_apply(scene, &Command::Tutorial(T::Leave), wall);
            }
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing mission clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        if matches!(command, Command::RefillEnergy) {
            let session = self.mission.as_ref().ok_or("Нет активного боя")?;
            if self.mission_paused()
                || session.battle.result.is_some()
                || session.battle.tutorial.is_some()
                || crate::story::current(scene, self).is_some()
            {
                return Err("Восстановление энергии сейчас недоступно".into());
            }
            return self.buy_special(scene, 4000, wall);
        }
        if matches!(command, Command::Retreat) {
            if let Some(session) = self.mission.as_ref().filter(|s| s.battle.pvp.is_some()) {
                if self.mission_paused()
                    || session.battle.phase != "battle"
                    || session.battle.result.is_some()
                    || session.battle.pvp_message(c).is_some()
                    || session.battle.pvp_score().is_some_and(|s| s.stars > 0)
                {
                    return Err("Отступление сейчас недоступно".into());
                }
                // Original0x64b91 differs from a timed/dead-army defeat:
                // finishTutorialPVP:NO, ActiveLost+1, no honor reduction,
                // delete battle and return directly, preserving stolen resources.
                let mut returned = self.clone();
                for u in session
                    .battle
                    .units
                    .iter()
                    .filter(|u| u.profile.is_some() && u.hp > 0)
                {
                    let uid = u.profile.as_ref().unwrap();
                    let entity = returned
                        .entities
                        .get_mut(uid)
                        .ok_or("Missing retreating troop archive")?;
                    entity["isDeleted"] = true.into();
                    entity["isInBattle"] = false.into();
                    if returned.selected.as_ref() == Some(uid) {
                        returned.selected = None;
                    }
                    returned.mission_receipt(
                        now,
                        session,
                        uid,
                        "mission_casualty",
                        BTreeMap::new(),
                    )?;
                }
                let p = returned.systems.as_mut().ok_or("Missing PvP profile")?;
                p.set(
                    "pvpActiveLost".into(),
                    p.integer("pvpActiveLost")
                        .checked_add(1)
                        .ok_or("PvP statistics overflow")?,
                );
                p.set("TutorialPVPComplete".into(), 0);
                returned.building_receipt(
                    now,
                    &session.id,
                    5990,
                    "pvp_retreat",
                    BTreeMap::new(),
                    vec![],
                )?;
                returned.mission = None;
                returned.validate(scene)?;
                return Ok((
                    returned,
                    "Отступление завершено; высаженные отряды потеряны".into(),
                ));
            }
            // Original confirmation callback0x64b91: deadAllSoldiers,
            // removeDeadSoldiersFromUser,deleteBattleState,returnToMainScreen.
            // Both receipts are published in one Profile commit; no invented
            // BattleEndDialog is inserted into this separate retreat branch.
            let (defeated, _) = self.mission_apply(scene, &Command::Surrender, wall)?;
            let (returned, _) = defeated.mission_apply(scene, &Command::Return, wall)?;
            return Ok((
                returned,
                "Отступление завершено; высаженные отряды потеряны".into(),
            ));
        }
        let mut next = self.clone();
        if let Command::Resume(name) = command {
            let session = self.mission.as_ref().ok_or("Нет сохранённого боя")?;
            if !self.mission_paused() || &session.battle.mission != name {
                return Err("Этот бой сейчас нельзя возобновить".into());
            }
            next.systems
                .as_mut()
                .ok_or("Нет игрового профиля")?
                .set("__native_mission_paused".into(), 0);
            next.building_receipt(
                now,
                &session.id,
                c.missions[name].mission_id,
                "mission_resume",
                BTreeMap::new(),
                vec![],
            )?;
            next.validate(scene)?;
            return Ok((next, "Бой продолжен".into()));
        }
        if self.mission_paused() {
            return Err("Сначала вернись в сохранённый бой".into());
        }
        if let Command::Open(name) = command {
            self.mission_access(scene, name)?;
            if self.mission.is_some() {
                return Err("Сначала закончи текущую миссию".into());
            }
            let m = c.missions.get(name).ok_or("Неизвестная миссия")?;
            if name == "tutorial_mission"
                && self.systems.as_ref().is_some_and(|p| {
                    p.flag("TutorialComplete")
                        || p.flag("tutorialCompleted")
                        || p.flag("Mission-5000")
                })
            {
                return Err("Обучение уже пройдено".into());
            }
            let roster = if name == "tutorial_mission" {
                // The original fresh archive already reserves two of its four starter
                // infantry for its tutorial. Reuse those identities, never mint
                // replacement troops or silently clear unrelated reservations.
                self.entities
                    .iter()
                    .filter(|(_, v)| number(v, "type") == 2001 && v["isDeleted"] != true)
                    .map(|(uid, _)| ArmyUnit {
                        uid: uid.clone(),
                        type_id: 2001,
                    })
                    .collect::<Vec<_>>()
            } else {
                self.mission_roster(scene)
            };
            if roster.is_empty() {
                return Err(crate::interface::lang(
                    scene,
                    "#nothing_soldiers",
                    "У тебя нет армии! Отправляйся на базу и начни тренировку войск.",
                )
                .into());
            }
            if roster.len() < m.min_descent {
                return Err(format!("Нужно отрядов: {}", m.min_descent));
            }
            let mut battle = Battle::new(c, name, roster)?;
            let mut initial_entropy = vec![];
            if battle.pvp.is_some() {
                battle.initialize_pvp(c, || {
                    let w = entropy()?;
                    initial_entropy.push(w);
                    Ok(w)
                })?;
            }
            if let Some(p) = &mut battle.pvp {
                p.finish_hint_acknowledged = self
                    .systems
                    .as_ref()
                    .and_then(|s| s.preferences.get("FirstPVPStar"))
                    .is_some_and(|v| !v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(1) != 0));
            }
            if scene
                .interface
                .as_ref()
                .is_some_and(|ui| ui.symbols.contains_key("story_balloon"))
            {
                battle.enable_story(c)?;
            }
            let id = format!("mission-{}-{}", m.mission_id, self.revision + 1);
            let session = Session {
                camera_serial: 0,
                camera: c.views.get(name).map_or([66., 646., 0.5], |v| v.camera),
                id: id.clone(),
                opened: now,
                reward_eligible: !self
                    .systems
                    .as_ref()
                    .ok_or("Missing preferences")?
                    .flag(&format!("Mission-{}", m.mission_id)),
                battle,
            };
            next.building_receipt(
                now,
                &id,
                m.mission_id,
                "mission_open",
                BTreeMap::new(),
                initial_entropy,
            )?;
            next.mission = Some(session);
            next.validate(scene)?;
            return Ok((next, format!("{}: размести войска в зоне высадки", m.title)));
        }
        let mut session = self.mission.clone().ok_or("Нет текущей миссии")?;
        if (session.battle.pvp_intro_active(c)
            || session
                .battle
                .pvp
                .as_ref()
                .is_some_and(|p| !p.messages.is_empty()))
            && !matches!(command, Command::PvpDialogue | Command::PvpTimeout)
        {
            return Err("Сначала закончи инструктаж".into());
        }
        if session.battle.tutorial.is_some()
            && matches!(
                command,
                Command::Suspend
                    | Command::Surrender
                    | Command::Retreat
                    | Command::CancelPlacement
                    | Command::Unplace(_)
            )
        {
            return Err("Сначала закончи обучение".into());
        }
        session.battle.visuals.clear();
        if session
            .battle
            .story
            .as_ref()
            .is_some_and(|s| s.message.is_some())
            && !matches!(command, Command::AdvanceDialogue)
        {
            return Err("Сначала закончи разговор".into());
        }
        let id = c.missions[&session.battle.mission].mission_id;
        if matches!(command, Command::Suspend) {
            if session.battle.phase != "battle" || session.battle.result.is_some() {
                return Err("Нет активного боя для возвращения на базу".into());
            }
            next.systems
                .as_mut()
                .ok_or("Нет игрового профиля")?
                .set("__native_mission_paused".into(), 1);
            next.building_receipt(
                now,
                &session.id,
                id,
                "mission_pause",
                BTreeMap::new(),
                vec![],
            )?;
            next.mission.as_mut().unwrap().battle.visuals.clear();
            next.validate(scene)?;
            return Ok((next, "Бой сохранён; войска остаются в сражении".into()));
        }
        if matches!(command, Command::CancelPlacement) {
            if session.battle.phase != "placement" {
                return Err(
                    "Бой уже начат; закрой окно, чтобы сохранить его и продолжить позже".into(),
                );
            }
            next.building_receipt(
                now,
                &session.id,
                id,
                "mission_cancel",
                BTreeMap::new(),
                vec![],
            )?;
            next.mission = None;
            next.validate(scene)?;
            return Ok((next, "Высадка отменена, армия не изменилась".into()));
        }
        if matches!(command, Command::Return) {
            if session.battle.result.is_none() {
                return Err("Миссия ещё продолжается".into());
            }
            next.building_receipt(
                now,
                &session.id,
                id,
                "mission_return",
                BTreeMap::new(),
                vec![],
            )?;
            next.mission = None;
            next.validate(scene)?;
            return Ok((next, "Возвращение на базу".into()));
        }
        let previous_result = session.battle.result.clone();
        let previous_energy = session.battle.energy;
        let (accepted, label) = match command {
            Command::PvpDialogue => {
                let accepted = session.battle.advance_pvp(c)?;
                if session
                    .battle
                    .pvp
                    .as_ref()
                    .is_some_and(|p| p.finish_hint_acknowledged)
                {
                    next.systems.as_mut().unwrap().set("FirstPVPStar".into(), 0);
                }
                (accepted, "Инструктаж продолжен")
            }
            Command::PvpTimeout => {
                if session.battle.pvp.is_none()
                    || session.battle.result.is_some()
                    || now - session.opened < 2700
                {
                    return Err("PvP timer has not expired".into());
                }
                let win = session.battle.pvp_score().is_some_and(|s| s.stars > 0);
                if !win {
                    for u in &mut session.battle.units {
                        if u.own && u.hp > 0 {
                            u.hp = 0;
                            u.own = false;
                        }
                    }
                }
                session.battle.pvp.as_mut().unwrap().messages.clear();
                session.battle.pvp.as_mut().unwrap().intro = c.pvp.as_ref().unwrap().steps.len();
                session.battle.result = Some(if win { "win" } else { "lose" }.into());
                (true, "Время боя истекло")
            }
            Command::FinishPvp => (session.battle.finish_pvp()?, "Бой завершён"),
            Command::Tutorial(action) => (
                session.battle.tutorial_apply(c, action)?,
                "Обучение продолжено",
            ),
            Command::AdvanceDialogue => {
                let message = session
                    .battle
                    .story
                    .as_ref()
                    .and_then(|s| s.message.as_ref())
                    .ok_or("Нет реплики")?;
                let count = crate::story::message_keys(scene, message).len();
                session.battle.advance_story(c, count)?;
                (true, "Разговор продолжен")
            }
            Command::Place(p, t) => {
                let placed = session.battle.place(c, *p, *t)?;
                if placed {
                    let unit = session
                        .battle
                        .units
                        .last_mut()
                        .ok_or("Missing placed troop")?;
                    let defs = self.upgraded_player_definitions(scene);
                    let d = defs
                        .get(&unit.id)
                        .ok_or("Missing player troop definition")?;
                    unit.hp = d.health;
                    unit.hp_max = d.health;
                    unit.attack = d.attack;
                    unit.distance = d.distance;
                    unit.speed = d.speed;
                }
                (placed, "Отряд размещён")
            }
            Command::Unplace(uid) => (
                session.battle.unplace(uid),
                "Отряд возвращён в список высадки",
            ),
            Command::Reposition(uid, p) => (
                session.battle.reposition(c, uid, *p),
                "Отряд перемещён в зоне высадки",
            ),
            Command::Start => {
                if session
                    .battle
                    .units
                    .iter()
                    .filter(|e| e.profile.is_some())
                    .count()
                    < c.missions[&session.battle.mission].min_descent
                {
                    return Err("Недостаточно отрядов для высадки".into());
                }
                session.battle.start(c)?;
                if let Some(p) = &mut session.battle.pvp {
                    p.messages
                        .push_back("#pvptutorial_1_need30persent_1_alice_right".into());
                }
                for u in session.battle.units.iter().filter(|u| u.profile.is_some()) {
                    let uid = u.profile.as_ref().unwrap();
                    let v = next
                        .entities
                        .get_mut(uid)
                        .ok_or("Missing deployed profile unit")?;
                    if v["isDeleted"] == true
                        || v["isInBattle"] == true && session.battle.tutorial.is_none()
                        || number(v, "type") as u32 != u.id
                    {
                        return Err("Отряд уже недоступен".into());
                    }
                    v["isInBattle"] = true.into();
                    next.mission_receipt(now, &session, uid, "mission_deploy", BTreeMap::new())?;
                }
                let p = next.systems.as_mut().unwrap();
                if session.battle.pvp.is_none() {
                    p.set(format!("MissionStarted-{id}"), 1);
                }
                if let Some(progression) = scene
                    .progression
                    .as_ref()
                    .filter(|_| session.battle.pvp.is_none())
                {
                    p.event(progression, next.resources["level"], 8, id)?;
                }
                (true, "Бой начат")
            }
            Command::Move(uid, p) => {
                let i = session.battle.index(uid).ok_or("Отряд не найден")?;
                (session.battle.player_move(c, i, *p)?, "Отряд перемещён")
            }
            Command::Attack(uid, target) => {
                let i = session.battle.index(uid).ok_or("Отряд не найден")?;
                let t = session.battle.index(target).ok_or("Цель не найдена")?;
                (session.battle.player_attack(c, i, t)?, "Атака завершена")
            }
            Command::AttackAll(target) => {
                let t = session.battle.index(target).ok_or("Цель не найдена")?;
                (session.battle.player_attack_all(c, t)?, "Атака завершена")
            }
            Command::AttackFrom(uid, target, p) => {
                let i = session.battle.index(uid).ok_or("Отряд не найден")?;
                let t = session.battle.index(target).ok_or("Цель не найдена")?;
                (
                    session.battle.player_attack_at(c, i, t, *p)?,
                    "Атака завершена",
                )
            }
            Command::Skip => (session.battle.skip(c)?, "Ход пропущен"),
            Command::Surrender => {
                if session.battle.phase != "battle" || session.battle.result.is_some() {
                    return Err("Нет активного боя".into());
                }
                // Explicit surrender = original actionMissionFail / deadAllSoldiers.
                // This is NOT the separate retreat-dialog branch.
                for u in &mut session.battle.units {
                    if u.own && u.hp > 0 {
                        u.hp = 0;
                        u.own = false;
                    }
                }
                session.battle.result = Some("lose".into());
                (true, "Миссия провалена: высаженные отряды потеряны")
            }
            _ => return Err("Недопустимая команда миссии".into()),
        };
        if !accepted {
            return Err("Действие недоступно: проверь дальность, свободную клетку и армию".into());
        }
        let cost = session
            .battle
            .energy
            .checked_sub(previous_energy)
            .ok_or("Invalid battle energy delta")?;
        next.spend_energy(scene, now, cost as i64)?;
        let mut gains = BTreeMap::new();
        // queueFinishMoveReturnToIdle0x52845 creates one XP FallingItem for
        // every nonenemy unit move, even in noBonuses tutorial5000. XP is
        // awarded by addFallingItemInBackground0x44c11 at spawn, not flight.
        // Commit it with the completed move so a restart cannot lose/replay it.
        // Original cosmetic spawn may be delayed0/1sec; presentation owns that
        // delay, while this atomic rules transaction represents completed spawn.
        for event in &session.battle.visuals {
            let crate::battle::VisualEvent::Move { uid, .. } = event else {
                continue;
            };
            let u =
                &session.battle.units[session.battle.index(uid).ok_or("Missing moving unit")?];
            if u.enemy || u.kind != "unit" || next.at_top_experience(scene) {
                continue;
            }
            crate::production::adjust(&mut next.resources, "exp", 1)?;
            next.building_receipt(
                now,
                uid,
                u.id,
                "mission_move_bonus",
                BTreeMap::from([("exp".into(), 1)]),
                vec![],
            )?;
            next.actions.last_mut().unwrap().source = Some(ProductionSource {
                uid: session.id.clone(),
                started: session.opened,
            });
        }
        if session.battle.pvp.is_none() && !c.missions[&session.battle.mission].no_bonuses {
            let mut health: BTreeMap<_, _> = self
                .mission
                .as_ref()
                .unwrap()
                .battle
                .units
                .iter()
                .map(|u| (u.uid.as_str(), u.hp))
                .collect();
            for event in &session.battle.visuals {
                let crate::battle::VisualEvent::Attack { target, damage, .. } = event else {
                    continue;
                };
                let u = &session.battle.units[session
                    .battle
                    .index(target)
                    .ok_or("Missing mission loot target")?];
                let hp = health.entry(target.as_str()).or_insert(u.hp + damage);
                let was_alive = *hp > 0;
                *hp -= damage;
                if !matches!(u.kind.as_str(), "building" | "decor") || !was_alive || *hp > 0 {
                    continue;
                }
                let mut loot = BTreeMap::new();
                let mut words = vec![];
                for drop in c.destroy_bonuses.get(&u.id).into_iter().flatten() {
                    let word = entropy()?;
                    words.push(word);
                    if word % 100 >= drop.probability {
                        continue;
                    }
                    next.award(
                        &crate::systems::Award {
                            kind: drop.kind.clone(),
                            count: drop.count.to_string(),
                            item_id: drop.item_id,
                        },
                        scene,
                        now,
                        &mut loot,
                    )?;
                }
                if !loot.is_empty() || !words.is_empty() {
                    next.building_receipt(now, target, u.id, "mission_destroy_bonus", loot, words)?;
                    next.actions.last_mut().unwrap().source = Some(ProductionSource {
                        uid: session.id.clone(),
                        started: session.opened,
                    });
                }
            }
        }
        if let Some(pvp) = session.battle.pvp.as_ref() {
            let old = self
                .mission
                .as_ref()
                .and_then(|s| s.battle.pvp.as_ref())
                .ok_or("Missing original PvP allocation")?;
            let before = old.stolen();
            let after = pvp.stolen();
            let mut loot_gains = BTreeMap::new();
            let mut loot_words = vec![];
            for r in 0..3 {
                let count = (after[r] - before[r]) as i64;
                if count < 0 {
                    return Err("PvP loot moved backwards".into());
                }
                if count > 0 {
                    crate::production::adjust(
                        &mut next.resources,
                        crate::pvp::RESOURCES[r],
                        count,
                    )?;
                    loot_gains.insert(crate::pvp::RESOURCES[r].into(), count);
                }
            }
            let mut health: BTreeMap<_, _> = self
                .mission
                .as_ref()
                .unwrap()
                .battle
                .units
                .iter()
                .map(|u| (u.uid.as_str(), u.hp))
                .collect();
            for event in &session.battle.visuals {
                let crate::battle::VisualEvent::Attack { target, damage, .. } = event else {
                    continue;
                };
                let u = &session.battle.units[session
                    .battle
                    .index(target)
                    .ok_or("Missing PvP loot target")?];
                let hp = health.entry(target).or_insert(u.hp + damage);
                let was_alive = *hp > 0;
                *hp -= damage;
                if u.kind != "building" || !was_alive || *hp > 0 {
                    continue;
                }
                if !matches!(u.id, 3001 | 3002 | 3010 | 3011) {
                    next.systems
                        .as_mut()
                        .ok_or("Missing achievement state")?
                        .achievement("destroy_buildings", 1, false);
                }
                for drop in c
                    .pvp
                    .as_ref()
                    .and_then(|p| p.destroy.get(&u.id))
                    .into_iter()
                    .flatten()
                {
                    let word = entropy()?;
                    loot_words.push(word);
                    if word % 100 >= drop.probability {
                        continue;
                    }
                    let award = crate::systems::Award {
                        kind: drop.kind.clone(),
                        count: drop.count.to_string(),
                        item_id: drop.item_id,
                    };
                    next.award(&award, scene, now, &mut loot_gains)?;
                }
            }
            if let Some(&gold) = loot_gains.get("playMoney") {
                next.systems
                    .as_mut()
                    .ok_or("Missing achievement state")?
                    .achievement("steal_playmoney", gold, false);
            }
            if !loot_gains.is_empty() || !loot_words.is_empty() {
                if let Some(config) = &scene.progression {
                    for r in crate::pvp::RESOURCES {
                        if let Some(&count) = loot_gains.get(r) {
                            next.systems
                                .as_mut()
                                .unwrap()
                                .pvp_resource(config, r, count)?;
                        }
                    }
                }
                next.building_receipt(now, &session.id, id, "pvp_loot", loot_gains, loot_words)?;
            }
        }
        next.update_level(scene, now, &mut gains)?;
        for u in session.battle.units.iter().filter(|u| u.profile.is_some()) {
            let uid = u.profile.as_ref().unwrap();
            let v = next
                .entities
                .get_mut(uid)
                .ok_or("Missing mission profile unit")?;
            if u.hp <= 0 && v["isDeleted"] != true {
                v["isDeleted"] = true.into();
                v["isInBattle"] = false.into();
                if next.selected.as_ref() == Some(uid) {
                    next.selected = None;
                }
                next.mission_receipt(now, &session, uid, "mission_casualty", BTreeMap::new())?;
            }
            if session.battle.result.is_some() {
                next.entities.get_mut(uid).unwrap()["isInBattle"] = false.into();
            }
        }
        if previous_result.is_none() && session.battle.result.is_some() {
            if session.battle.pvp.is_some() {
                let win = session.battle.result.as_deref() == Some("win");
                let stars = if win {
                    session.battle.pvp_score().unwrap().stars as usize
                } else {
                    0
                };
                let honor = [-100i64, 33, 67, 100][stars];
                let p = next.systems.as_mut().ok_or("Missing PvP profile")?;
                let key = if win {
                    "pvpActiveWins"
                } else {
                    "pvpActiveLost"
                };
                p.set(
                    key.into(),
                    p.integer(key)
                        .checked_add(1)
                        .ok_or("PvP statistics overflow")?,
                );
                p.set(
                    "honor".into(),
                    p.integer("honor")
                        .checked_add(honor)
                        .ok_or("PvP honor overflow")?
                        .max(0),
                );
                p.achievement("achieve_combat_points", honor, false);
                p.set("TutorialPVPComplete".into(), 1);
            } else if session.battle.result.as_deref() == Some("win") {
                // Completed maps are an explicit native replay with no repeat
                // first-completion rewards; never turn imported completion into cash.
                if session.reward_eligible {
                    for award in &c.missions[&session.battle.mission].awards {
                        next.award(award, scene, now, &mut gains)?;
                    }
                    next.update_level(scene, now, &mut gains)?;
                    let p = next.systems.as_mut().unwrap();
                    p.set(format!("Mission-{id}"), 1);
                    if let Some(config) = &scene.progression {
                        p.event(config, next.resources["level"], 9, id)?;
                    }
                    let total = p
                        .preferences
                        .iter()
                        .filter(|(k, _)| k.starts_with("Mission-") && p.flag(k))
                        .count() as i64;
                    p.achievement("complete_missions", total, true);
                }
            }
            let kind = if session.battle.result.as_deref() == Some("win") {
                "mission_win"
            } else {
                "mission_defeat"
            };
            next.building_receipt(now, &session.id, id, kind, gains, vec![])?;
        } else {
            next.building_receipt(now, &session.id, id, "mission_action", gains, vec![])?;
        }
        if matches!(
            command,
            Command::Tutorial(crate::battle::TutorialCommand::Leave)
        ) {
            for r in &session.battle.roster {
                if let Some(v) = next.entities.get_mut(&r.uid) {
                    v["isInBattle"] = false.into();
                }
            }
            next.building_receipt(
                now,
                &session.id,
                id,
                "mission_return",
                BTreeMap::new(),
                vec![],
            )?;
            next.mission = None;
        } else {
            next.mission = Some(session);
        }
        next.validate(scene)?;
        Ok((next, label.into()))
    }
    pub fn validate_mission(&self, scene: &Scene) -> Result<(), String> {
        let mut destroyed = std::collections::BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "mission_destroy_bonus")
        {
            let source = a
                .source
                .as_ref()
                .ok_or("Missing destruction bonus provenance")?;
            let c = scene
                .combat
                .as_ref()
                .ok_or("Missing destruction bonus config")?;
            let open = self
                .actions
                .iter()
                .find(|o| {
                    o.kind == "mission_open"
                        && o.uid == source.uid
                        && o.at == source.started
                        && o.sequence < a.sequence
                })
                .ok_or("Missing destruction bonus mission")?;
            let m = c
                .missions
                .values()
                .find(|m| m.mission_id == open.recipe)
                .ok_or("Unknown destruction bonus mission")?;
            let drops = c
                .destroy_bonuses
                .get(&a.recipe)
                .ok_or("Unknown destruction reward target")?;
            if m.no_bonuses
                || !m
                    .entities
                    .iter()
                    .any(|e| e.uid == a.uid && e.id == a.recipe)
                || a.entropy.len() != drops.len()
                || !destroyed.insert((source.uid.clone(), a.uid.clone()))
                || !a.quest_events_pending.is_empty()
                || !self.actions.iter().any(|e| {
                    e.uid == source.uid
                        && e.sequence > a.sequence
                        && e.at == a.at
                        && matches!(
                            e.kind.as_str(),
                            "mission_action" | "mission_win" | "mission_defeat"
                        )
                })
            {
                return Err("Invalid destruction bonus receipt".into());
            }
            let mut expected = BTreeMap::<String, i64>::new();
            for (drop, word) in drops.iter().zip(&a.entropy) {
                if word % 100 >= drop.probability {
                    continue;
                }
                let key = match drop.kind.as_str() {
                    "experience" => "exp".into(),
                    "item" => format!("item:{}", drop.item_id),
                    k => k.into(),
                };
                *expected.entry(key).or_default() += drop.count as i64;
            }
            // Top-level profiles may legitimately suppress XP in award().
            expected.retain(|_, n| *n != 0);
            if a.gains != expected
                && !(self.at_top_experience(scene) && {
                    expected.remove("exp");
                    a.gains == expected
                })
            {
                return Err("Destruction bonus differs from original RNG".into());
            }
        }
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind == "mission_move_bonus")
        {
            let source = a
                .source
                .as_ref()
                .ok_or("Missing battle movement bonus provenance")?;
            let valid_open = self.actions.iter().any(|o| {
                o.kind == "mission_open"
                    && o.uid == source.uid
                    && o.at == source.started
                    && o.sequence < a.sequence
            });
            let valid_end = self.actions.iter().any(|e| {
                e.uid == source.uid
                    && e.sequence > a.sequence
                    && e.at == a.at
                    && matches!(
                        e.kind.as_str(),
                        "mission_action" | "mission_win" | "mission_defeat"
                    )
            });
            if !valid_open
                || !valid_end
                || a.gains != BTreeMap::from([("exp".into(), 1)])
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
                || !scene
                    .combat
                    .as_ref()
                    .is_some_and(|c| c.defs.get(&a.recipe).is_some_and(|d| d.group == "units"))
            {
                return Err("Invalid battle movement XP receipt".into());
            }
        }
        if self.mission_paused()
            && self
                .mission
                .as_ref()
                .is_none_or(|s| s.battle.phase != "battle" || s.battle.result.is_some())
        {
            return Err("Invalid paused mission state".into());
        }
        if let Some(session) = &self.mission {
            let paused_receipt = self
                .actions
                .iter()
                .rev()
                .find(|a| {
                    a.uid == session.id
                        && ["mission_pause", "mission_resume"].contains(&a.kind.as_str())
                })
                .is_some_and(|a| a.kind == "mission_pause");
            if paused_receipt != self.mission_paused() {
                return Err("Mission visibility differs from its receipt".into());
            }
        }
        if let Some(s) = &self.mission {
            let c = scene
                .combat
                .as_ref()
                .ok_or("Saved mission needs a combat package")?;
            s.battle
                .validate_with_player(c, &self.upgraded_player_definitions(scene))?;
            if let Some(pvp) = &s.battle.pvp {
                let opened = self
                    .actions
                    .iter()
                    .find(|a| a.kind == "mission_open" && a.uid == s.id)
                    .ok_or("Missing PvP opening receipt")?;
                if opened.entropy != pvp.entropy {
                    return Err("PvP allocation entropy differs from opening receipt".into());
                }
                let mut received = [0i64; 3];
                for a in self
                    .actions
                    .iter()
                    .filter(|a| a.kind == "pvp_loot" && a.uid == s.id)
                {
                    if a.recipe != 5990
                        || a.at < s.opened
                        || a.entropy.len() > 64
                        || !a.quest_events_pending.is_empty()
                    {
                        return Err("Invalid PvP loot provenance".into());
                    }
                    for (r, key) in crate::pvp::RESOURCES.iter().enumerate() {
                        received[r] = received[r]
                            .checked_add(*a.gains.get(*key).unwrap_or(&0))
                            .ok_or("PvP loot receipt overflow")?;
                    }
                }
                if received != pvp.stolen().map(i64::from) {
                    return Err("PvP loot and committed wallet receipts diverge".into());
                }
            }
            if let Some(m) = s.battle.story.as_ref().and_then(|s| s.message.as_ref()) {
                if m.phrase >= crate::story::message_keys(scene, m).len() {
                    return Err("Saved dialogue phrase is outside its source sequence".into());
                }
            }
            if !s.camera.iter().all(|v| v.is_finite())
                || !(0.12..=2.).contains(&s.camera[2])
                || s.camera_serial > s.battle.camera_request.as_ref().map_or(0, |r| r.serial)
            {
                return Err("Invalid mission camera".into());
            }
            if s.opened <= 0
                || s.opened > self.last_action_time
                || self
                    .actions
                    .iter()
                    .filter(|a| {
                        a.kind == "mission_open"
                            && a.uid == s.id
                            && a.at == s.opened
                            && a.recipe == c.missions[&s.battle.mission].mission_id
                    })
                    .count()
                    != 1
            {
                return Err("Invalid mission start receipt".into());
            }
            for u in s.battle.units.iter().filter(|u| u.profile.is_some()) {
                let uid = u.profile.as_ref().unwrap();
                let v = self
                    .entities
                    .get(uid)
                    .ok_or("Missing mission unit archive")?;
                if number(v, "type") as u32 != u.id
                    || v["isDeleted"] == true && u.hp > 0
                    || self.unit_in_mission(uid) != (v["isInBattle"] == true)
                        && !(s.battle.tutorial.is_some()
                            && s.battle.phase == "placement"
                            && scene
                                .entities
                                .iter()
                                .any(|e| e.uid == *uid && e.archived["isInBattle"] == true))
                {
                    return Err("Mission/profile unit status diverges".into());
                }
            }
        }
        for (uid, v) in &self.entities {
            if v["isInBattle"] == true && !self.unit_in_mission(uid) {
                let imported = scene
                    .entities
                    .iter()
                    .any(|e| e.uid == *uid && e.archived["isInBattle"] == true);
                if !imported {
                    return Err("Orphaned mission reservation".into());
                }
            }
        }
        let mut reservations = BTreeSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "mission_deploy") {
            let source = a
                .source
                .as_ref()
                .ok_or("Missing mission reservation provenance")?;
            let entity = self
                .entities
                .get(&a.uid)
                .ok_or("Missing reserved troop archive")?;
            let open = self
                .actions
                .iter()
                .filter(|o| {
                    o.kind == "mission_open"
                        && o.uid == source.uid
                        && o.at == source.started
                        && o.sequence < a.sequence
                })
                .collect::<Vec<_>>();
            if open.len() != 1
                || !scene
                    .combat
                    .as_ref()
                    .is_some_and(|c| c.missions.values().any(|m| m.mission_id == open[0].recipe))
                || !reservations.insert((&a.uid, &source.uid))
                || a.recipe != number(entity, "type") as u32
                || !(2000..3000).contains(&a.recipe)
                || source.started > a.at
                || a.at > self.last_action_time
                || !a.gains.is_empty()
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid native mission reservation receipt".into());
            }
        }
        let mut casualties = BTreeSet::new();
        for a in self.actions.iter().filter(|a| a.kind == "mission_casualty") {
            let source = a.source.as_ref().ok_or("Missing casualty provenance")?;
            let deployed = self.actions.iter().any(|d| {
                d.kind == "mission_deploy"
                    && d.uid == a.uid
                    && d.sequence < a.sequence
                    && d.source
                        .as_ref()
                        .is_some_and(|s| s.uid == source.uid && s.started == source.started)
            });
            if !casualties.insert(&a.uid)
                || !deployed
                || !a.gains.is_empty()
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
                || a.at > self.last_action_time
                || self
                    .actions
                    .iter()
                    .any(|later| later.uid == a.uid && later.sequence > a.sequence)
                || self
                    .entities
                    .get(&a.uid)
                    .is_none_or(|v| v["isDeleted"] != true || v["isInBattle"] != false)
            {
                return Err("Invalid casualty receipt".into());
            }
        }
        Ok(())
    }
}
