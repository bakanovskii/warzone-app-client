//! Source Tutorials.prepareData and SceneBattle tutorial input branches.
//! Coordinates are source map cells (the battle collision map adds OFFSET).
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TutorialStep {
    #[serde(rename = "iD")]
    pub id: i32,
    #[serde(default, rename = "andAction")]
    pub action: i32,
    #[serde(default, rename = "placeX")]
    pub x: i32,
    #[serde(default, rename = "placeY")]
    pub y: i32,
    #[serde(default, rename = "unitID")]
    pub unit: String,
    #[serde(default, rename = "useHole")]
    pub hole: bool,
    #[serde(default, rename = "useDark")]
    pub dark: bool,
    #[serde(default, rename = "useArrow")]
    pub arrow: bool,
    #[serde(default, rename = "btnText")]
    pub button: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub screen: bool,
}
impl TutorialStep {
    pub fn point(&self) -> [i32; 2] {
        [self.x, self.y]
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tutorial {
    pub index: usize,
    pub selected: Option<String>,
    pub route: Option<[i32; 2]>,
    pub enemy: Option<String>,
}
#[derive(Clone, Debug)]
pub enum TutorialCommand {
    Okay,
    Select(String),
    Route([i32; 2]),
    Enemy(String),
    DismissResult,
    Leave,
}
impl Battle {
    pub fn tutorial_step<'a>(&self, c: &'a Config) -> Option<&'a TutorialStep> {
        c.tutorial_steps.get(self.tutorial.as_ref()?.index)
    }
    pub fn tutorial_id(&self, c: &Config) -> i32 {
        self.tutorial_step(c).map_or(-1, |s| s.id)
    }
    pub(super) fn tutorial_next(&mut self, c: &Config) -> Result<(), String> {
        let Some(t) = &mut self.tutorial else {
            return Ok(());
        };
        t.index += 1;
        while let Some(s) = self.tutorial_step(c).filter(|s| s.action != 0) {
            let point = s.point();
            let milliseconds = if s.action == 1 { 2000 } else { 1000 };
            self.camera_request = Some(crate::battle_camera::Request {
                serial: self.camera_request.as_ref().map_or(1, |r| r.serial + 1),
                point,
                milliseconds,
            });
            self.tutorial.as_mut().unwrap().index += 1;
        }
        Ok(())
    }
    pub fn tutorial_apply(
        &mut self,
        c: &Config,
        command: &TutorialCommand,
    ) -> Result<bool, String> {
        let Some(step) = self.tutorial_step(c).cloned() else {
            return Ok(false);
        };
        if step.action != 0
            || self.result.is_some()
                && !matches!(
                    command,
                    TutorialCommand::DismissResult | TutorialCommand::Leave
                )
        {
            return Ok(false);
        }
        match command {
            TutorialCommand::Okay => {
                if step.button.is_empty() {
                    return Ok(false);
                }
                // Source OK block0x13555c: ID8 calls battleCheckResults,
                // does NOT advance. Result dialog dismissal advances to ID9.
                if step.id == 8 {
                    self.check_results(c, 0)?;
                } else {
                    self.tutorial_next(c)?;
                }
            }
            TutorialCommand::Select(uid) => {
                if !step.button.is_empty() {
                    return Ok(false);
                }
                let Some(i) = self
                    .index(uid)
                    .filter(|i| self.units[*i].own && self.alive(*i))
                else {
                    return Ok(false);
                };
                if [5, 101].contains(&step.id)
                    && self.units[i].cell() != [step.x + OFFSET, step.y + OFFSET]
                {
                    return Ok(false);
                }
                let t = self.tutorial.as_mut().unwrap();
                t.selected = Some(uid.clone());
                t.route = None;
                t.enemy = None;
                if [5, 101].contains(&step.id) {
                    self.tutorial_next(c)?;
                }
            }
            TutorialCommand::Route(p) => {
                if !step.button.is_empty()
                    || ![105, 102, 106, 107].contains(&step.id)
                    || *p != step.point()
                {
                    return Ok(false);
                }
                let Some(i) = self
                    .tutorial
                    .as_ref()
                    .and_then(|t| t.selected.as_ref())
                    .and_then(|u| self.index(u))
                else {
                    return Ok(false);
                };
                let end = [p[0] + OFFSET, p[1] + OFFSET];
                let u = &self.units[i];
                if !Self::free(&self.collisions(c), end)
                    || trace(&self.collisions(c), u.cell(), end, u.speed, u.id)
                        .is_none_or(|r| r.len() < 2 || r.len() - 1 > u.speed as usize)
                {
                    return Ok(false);
                }
                self.tutorial.as_mut().unwrap().route = Some(*p);
                if [105, 102].contains(&step.id) {
                    self.tutorial_next(c)?;
                }
            }
            TutorialCommand::Enemy(uid) => {
                if !step.button.is_empty() {
                    return Ok(false);
                }
                let Some(i) = self
                    .index(uid)
                    .filter(|i| self.units[*i].enemy && self.alive(*i))
                else {
                    return Ok(false);
                };
                self.tutorial.as_mut().unwrap().enemy = Some(uid.clone());
                if [6, 7].contains(&step.id)
                    && (step.unit == *uid
                        || self.units[i].cell() == [step.x + OFFSET, step.y + OFFSET])
                {
                    self.tutorial_next(c)?;
                }
            }
            TutorialCommand::DismissResult => {
                if step.id != 8 || self.result.as_deref() != Some("win") {
                    return Ok(false);
                }
                self.tutorial_next(c)?;
            }
            TutorialCommand::Leave => {
                // This is a battle-part boundary only. Completion of the whole
                // base tutorial and its statistics is handled separately.
                if step.id != 9 || self.result.as_deref() != Some("win") {
                    return Ok(false);
                }
                self.tutorial.as_mut().unwrap().index = c.tutorial_steps.len();
                self.result = Some("win".into());
            }
        }
        Ok(true)
    }
    pub(super) fn tutorial_moved(&mut self, c: &Config, u: usize) -> Result<(), String> {
        let id = self.tutorial_id(c);
        if self.units[u].own && [106, 107, 102].contains(&id) {
            self.tutorial.as_mut().unwrap().selected = Some(self.units[u].uid.clone());
            self.tutorial_next(c)?;
        } else if self.units[u].enemy && id == 61 {
            self.tutorial_next(c)?;
        }
        Ok(())
    }
    pub(super) fn tutorial_enemy_move(&mut self, c: &Config) {
        if [106, 0].contains(&self.tutorial_id(c)) {
            return;
        }
        let own = self.own();
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if u.kind != "unit"
                || !u.enemy
                || !self.alive(i)
                || u.mode != 2
                || !self.queue_can_move(i)
            {
                continue;
            }
            let nearest = own
                .iter()
                .copied()
                .min_by_key(|s| self.units[i].distance_to(&self.units[*s]));
            if let Some(s) = nearest {
                if self.units[i].range() < self.units[i].distance_to(&self.units[s])
                    && self.enemy_move_to(c, i, self.units[s].cell())
                {
                    return;
                }
            }
        }
    }
    pub(super) fn validate_tutorial(&self, c: &Config) -> Result<(), String> {
        if let Some(t) = &self.tutorial {
            if self.mission != "tutorial_mission"
                || c.tutorial_steps.len() != 27
                || t.index > 27
                || t.index >= 26 && self.result.as_deref() != Some("win")
                || self.result.as_ref().is_some_and(|r| {
                    r != "win" || t.index < 25 || self.units.iter().any(|u| u.enemy && u.hp > 0)
                })
                || t.selected.as_ref().is_some_and(|u| {
                    self.index(u)
                        .is_none_or(|i| !self.units[i].own || !self.alive(i))
                })
                || t.enemy.as_ref().is_some_and(|u| {
                    self.index(u)
                        .is_none_or(|i| !self.units[i].enemy || !self.alive(i))
                })
                || t.route.is_some_and(|p| {
                    self.tutorial_step(c)
                        .is_none_or(|s| ![106, 107].contains(&s.id) || s.point() != p)
                })
                || self.story.is_some()
            {
                return Err("Invalid saved tutorial continuation".into());
            }
        } else if self.mission == "tutorial_mission" {
            return Err("Tutorial continuation missing".into());
        }
        Ok(())
    }
}
