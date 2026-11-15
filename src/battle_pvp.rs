//! Original offline NPC5990 arena. Separate from mission AI and mission awards.
use super::*;
use crate::pvp;
pub const NPC_RESOURCES: [i32; 3] = [3000, 100, 90];
impl Battle {
    pub fn pvp_intro_active(&self, c: &Config) -> bool {
        self.pvp
            .as_ref()
            .is_some_and(|p| p.intro < c.pvp.as_ref().map_or(0, |c| c.steps.len()))
    }
    pub fn pvp_step<'a>(&self, c: &'a Config) -> Option<&'a pvp::Step> {
        c.pvp.as_ref()?.steps.get(self.pvp.as_ref()?.intro)
    }
    pub fn pvp_message<'a>(&'a self, c: &'a Config) -> Option<&'a str> {
        if self.result.is_some() {
            return None;
        }
        self.pvp_step(c)
            .map(|s| s.text.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| self.pvp.as_ref()?.messages.front().map(String::as_str))
    }
    pub fn initialize_pvp(
        &mut self,
        c: &Config,
        mut entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<(), String> {
        let config = c.pvp.as_ref().ok_or("Нет данных PvP-обучения")?;
        if self.mission != "pvptutorial_1" || self.pvp.as_ref().is_none_or(|p| !p.loot.is_empty()) {
            return Err("PvP arena already initialized or unavailable".into());
        }
        let mut words = vec![];
        let loot = config.loot.distribute(&self.units, NPC_RESOURCES, 1., || {
            let w = entropy()?;
            words.push(w);
            Ok(w)
        })?;
        let p = self.pvp.as_mut().unwrap();
        p.loot = loot;
        p.entropy = words;
        // The scenario's sole BeginPlaceUnits trigger sets the guard's AI mode.
        self.fire(c, 205, json!({}), 0)?;
        Ok(())
    }
    pub fn advance_pvp(&mut self, c: &Config) -> Result<bool, String> {
        let Some(p) = self.pvp.as_mut() else {
            return Ok(false);
        };
        if self.phase == "battle" {
            if let Some(dismissed) = p.messages.pop_front() {
                if dismissed == "#pvp_end_game_short_info" {
                    p.finish_hint_acknowledged = true;
                }
                if dismissed == "#pvptutorial_1_1star_3_alice_left" {
                    let z = c.missions[&self.mission]
                        .zones
                        .iter()
                        .find(|z| z.name == "hq_camera")
                        .ok_or("Missing original headquarters camera")?;
                    self.camera_request = Some(crate::battle_camera::Request {
                        serial: self.camera_request.as_ref().map_or(1, |r| r.serial + 1),
                        point: [z.x + z.w / 2, z.y + z.h / 2],
                        milliseconds: 2000,
                    });
                }
                self.pvp_results()?;
                return Ok(true);
            }
        }
        let config = c.pvp.as_ref().ok_or("Missing PvP tutorial")?;
        if p.intro >= config.steps.len() || self.phase != "placement" || self.result.is_some() {
            return Ok(false);
        }
        p.intro += 1;
        while let Some(step) = self.pvp_step(c).filter(|s| s.action == 1) {
            let name = step
                .directive
                .strip_prefix("scroll:")
                .ok_or("Unsupported PvP camera directive")?;
            let z = c.missions[&self.mission]
                .zones
                .iter()
                .find(|z| z.name == name)
                .ok_or("Missing PvP tutorial zone")?;
            self.camera_request = Some(crate::battle_camera::Request {
                serial: self.camera_request.as_ref().map_or(1, |r| r.serial + 1),
                point: [z.x + z.w / 2, z.y + z.h / 2],
                milliseconds: 2000,
            });
            self.pvp.as_mut().unwrap().intro += 1;
        }
        Ok(true)
    }
    pub fn pvp_score(&self) -> Option<pvp::Score> {
        Some(pvp::score(
            &self.units,
            self.pvp.as_ref()?.headquarters_destroyed,
        ))
    }
    pub(super) fn pvp_results(&mut self) -> Result<(), String> {
        if self.phase != "battle" || self.result.is_some() {
            return Ok(());
        }
        let score = self.pvp_score().ok_or("Missing PvP score")?;
        self.pvp.as_mut().unwrap().headquarters_destroyed = score.headquarters;
        if self.own().is_empty() {
            self.pvp.as_mut().unwrap().messages.clear();
            self.result = Some("lose".into());
            return Ok(());
        }
        if self.pvp.as_ref().is_some_and(|p| !p.messages.is_empty()) {
            return Ok(());
        }
        let changed = score.stars != self.pvp.as_ref().unwrap().last_stars;
        self.pvp.as_mut().unwrap().last_stars = score.stars;
        if score.stars == 3 {
            self.result = Some("win".into());
            return Ok(());
        }
        if changed && score.stars == 1 && !self.pvp.as_ref().unwrap().first_star_shown {
            let p = self.pvp.as_mut().unwrap();
            p.first_star_shown = true;
            p.messages = [
                "#pvptutorial_1_1star_1_alice_left",
                "#pvptutorial_1_1star_2_skalov_right",
                "#pvptutorial_1_1star_3_alice_left",
                "#pvptutorial_1_1star_4_alice_left",
            ]
            .into_iter()
            .map(String::from)
            .collect();
            return Ok(());
        }
        if changed && score.stars > 0 && !self.pvp.as_ref().unwrap().finish_hint_acknowledged {
            self.pvp
                .as_mut()
                .unwrap()
                .messages
                .push_back("#pvp_end_game_short_info".into());
        }
        Ok(())
    }
    pub fn finish_pvp(&mut self) -> Result<bool, String> {
        if self.phase != "battle"
            || self.result.is_some()
            || self.own().is_empty()
            || self
                .pvp
                .as_ref()
                .is_some_and(|p| !p.messages.is_empty() || !p.finish_hint_acknowledged)
            || self.pvp_score().is_none_or(|s| s.stars == 0)
        {
            return Ok(false);
        }
        self.result = Some("win".into());
        Ok(true)
    }
    pub(super) fn pvp_camera_valid(&self, c: &Config, r: &crate::battle_camera::Request) -> bool {
        self.pvp
            .as_ref()
            .zip(c.pvp.as_ref())
            .is_some_and(|(p, config)| {
                config
                    .steps
                    .iter()
                    .take(p.intro)
                    .filter_map(|s| s.directive.strip_prefix("scroll:"))
                    .chain(p.first_star_shown.then_some("hq_camera"))
                    .any(|name| {
                        c.missions[&self.mission].zones.iter().any(|z| {
                            z.name == name
                                && [z.x + z.w / 2, z.y + z.h / 2] == r.point
                                && r.milliseconds == 2000
                        })
                    })
            })
    }
    pub(super) fn validate_pvp(&self, c: &Config) -> Result<(), String> {
        if (self.mission == "pvptutorial_1") != self.pvp.is_some() {
            return Err("PvP/mission mode mismatch".into());
        }
        let Some(p) = &self.pvp else { return Ok(()) };
        let config = c.pvp.as_ref().ok_or("Saved PvP needs source data")?;
        let allowed = [
            "#pvptutorial_1_need30persent_1_alice_right",
            "#pvptutorial_1_1star_1_alice_left",
            "#pvptutorial_1_1star_2_skalov_right",
            "#pvptutorial_1_1star_3_alice_left",
            "#pvptutorial_1_1star_4_alice_left",
            "#pvp_end_game_short_info",
        ];
        if self.tutorial.is_some()
            || self.story.is_some()
            || p.last_stars > 3
            || p.intro > config.steps.len()
            || self.phase == "battle" && p.intro != config.steps.len()
            || p.entropy.len() > 4096
            || p.attackers.len() > 1000
            || p.messages.len() > 4
            || p.messages.iter().any(|s| !allowed.contains(&s.as_str()))
            || self.phase == "placement" && !p.messages.is_empty()
        {
            return Err("Invalid saved PvP continuation".into());
        }
        let seed = Battle::new(c, &self.mission, self.roster.clone())?;
        let mut words = p.entropy.iter();
        let initial = config.loot.distribute(&seed.units, NPC_RESOURCES, 1., || {
            words
                .next()
                .copied()
                .ok_or("Missing original PvP allocation entropy".into())
        })?;
        if words.next().is_some() || p.loot.len() != initial.len() {
            return Err("Invalid PvP allocation transcript".into());
        }
        for (l, expected) in p.loot.iter().zip(initial) {
            if l.uid != expected.uid
                || (0..3).any(|r| {
                    l.contents[r] < 0
                        || l.stolen[r] < 0
                        || l.contents[r].checked_add(l.stolen[r]) != Some(expected.contents[r])
                })
            {
                return Err("PvP loot lost its source allocation".into());
            }
        }
        if p.headquarters_destroyed != self.units.iter().any(|u| u.id == 1000 && u.hp <= 0) {
            return Err("Invalid PvP headquarters marker".into());
        }
        let mut ids = BTreeSet::new();
        for uid in &p.attackers {
            if !ids.insert(uid)
                || self
                    .index(uid)
                    .is_none_or(|i| self.units[i].kind != "unit" || self.units[i].enemy)
            {
                return Err("Invalid persistent PvP attacker identity".into());
            }
        }
        if self.result.as_deref() == Some("win") && self.pvp_score().is_none_or(|s| s.stars == 0) {
            return Err("PvP victory without a star".into());
        }
        Ok(())
    }
    /// enemyAttackAtThisMoment0x5bdfd and enemyNextTarget0x5c1b1. Source first
    /// chooses the lowest-health in-range victim for each attacker, then prefers
    /// a kill with the largest victim HP, else the least HP after focus fire.
    fn pvp_next_target(&mut self) -> Option<usize> {
        let (e, b) = self.attackers();
        let attackers: Vec<_> = e.into_iter().chain(b).collect();
        let own = self.own();
        for &i in &attackers {
            self.units[i].target = None;
            if !self.alive(i) {
                continue;
            }
            self.units[i].mode = 1;
            let mut target: Option<usize> = None;
            for &s in &own {
                if self.units[s].hp > 0
                    && self.units[s].distance_to(&self.units[i]) <= self.units[i].range()
                    && target.is_none_or(|old| self.units[s].hp < self.units[old].hp)
                {
                    target = Some(s);
                }
            }
            self.units[i].target = target.map(|s| self.units[s].uid.clone());
        }
        let mut lethal = vec![];
        let mut nonlethal = vec![];
        let mut weakest = None;
        let mut remaining = 200;
        for &i in &attackers {
            if !self.alive(i) {
                continue;
            }
            let Some(t) = self.units[i].target.as_ref().and_then(|s| self.index(s)) else {
                continue;
            };
            if self.units[i].mode != 1 {
                continue;
            }
            let hp = self.units[t].hp - self.units[i].damage_to(&self.units[t]);
            if hp <= 0 {
                lethal.push(t)
            } else {
                nonlethal.push(t);
                if hp < remaining {
                    weakest = Some(t);
                    remaining = hp;
                }
            }
        }
        for t in nonlethal {
            let damage = attackers
                .iter()
                .filter(|&&i| {
                    self.alive(i)
                        && self.units[t].distance_to(&self.units[i]) <= self.units[i].range()
                })
                .map(|&i| self.units[i].damage_to(&self.units[t]))
                .sum::<i32>();
            let hp = self.units[t].hp - damage;
            if hp < 0 {
                lethal.push(t)
            } else if hp < remaining {
                weakest = Some(t);
                remaining = hp;
            }
        }
        if lethal.is_empty() {
            weakest
        } else {
            let mut chosen = None;
            let mut max = 0;
            for t in lethal {
                if self.units[t].hp > max {
                    max = self.units[t].hp;
                    chosen = Some(t)
                }
            }
            chosen
        }
    }
    pub(super) fn pvp_enemy_turn(&mut self, c: &Config) {
        if let Some(t) = self.pvp_next_target() {
            let uid = self.units[t].uid.clone();
            let (e, b) = self.attackers();
            for i in e.into_iter().chain(b) {
                if self.units[i].target.as_ref() != Some(&uid) {
                    self.units[i].target = None;
                }
            }
            self.enemy_do_attack(c);
            return;
        }
        // Original5990has one guard and no offensive structures. Keep that source
        // constraint explicit rather than pretending this proves arbitrary server
        // defenses, multi-guard shortest-route sorting or upgraded enemy models.
        let own = self.own();
        let mut threatening = vec![];
        for building in self
            .units
            .iter()
            .filter(|u| u.kind == "building" && u.hp > 0 && !matches!(u.id, 1026 | 1028))
        {
            for &s in &own {
                if self.alive(s)
                    && self.units[s].distance_to(building) <= self.units[s].range()
                    && !threatening.contains(&s)
                {
                    threatening.push(s);
                }
            }
        }
        if threatening.is_empty() {
            return;
        }
        for s in threatening {
            let uid = self.units[s].uid.clone();
            let list = &mut self.pvp.as_mut().unwrap().attackers;
            if !list.contains(&uid) {
                list.push(uid);
            }
        }
        let visitors = self.pvp.as_ref().unwrap().attackers.clone();
        let guards = self.attackers().0;
        for i in guards {
            if !self.alive(i) {
                continue;
            }
            self.units[i].mode = 2;
            let mut target: Option<usize> = None;
            for uid in &visitors {
                let Some(s) = self.index(uid) else { continue };
                if target.is_none_or(|t| {
                    self.units[t].hp < 1
                        || self.units[s].distance_to(&self.units[i])
                            < self.units[t].distance_to(&self.units[i])
                }) {
                    target = Some(s);
                }
            }
            if let Some(t) = target {
                self.units[i].target = Some(self.units[t].uid.clone());
                self.move_min_distance(c, i, t);
                return;
            }
        }
    }
}
