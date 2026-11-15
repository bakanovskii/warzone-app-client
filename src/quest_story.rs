//! SceneMain.questDisplayInfo0x9a561, QuestItem.getPersonageTexts0x29765,
//! Quests.isQuestMessageNew0x292d1 and PersonagesQueue. No server/runtime calls.
//! Keep an unfinished phrase in the private native preferences so reopening the
//! application cannot silently discard the rest of an already acknowledged intro.
use crate::{Scene, production::State};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
const KEY: &str = "__native_quest_story";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub quest: u32,
    pub phrase: usize,
}
pub fn keys(scene: &Scene, quest: u32) -> Vec<String> {
    crate::story::phrase_keys(scene, &format!("#QUEST_{quest}_DIALOG"))
        .into_iter()
        .filter(|k| {
            scene
                .progression
                .as_ref()
                .is_some_and(|p| p.language.contains_key(k))
        })
        .collect()
}
impl State {
    pub fn quest_story(&self) -> Result<Option<Pending>, String> {
        self.systems
            .as_ref()
            .and_then(|s| s.preferences.get(KEY))
            .map(|v| {
                serde_json::from_value(v.clone()).map_err(|_| "Invalid quest dialogue queue".into())
            })
            .transpose()
    }
    pub fn quest_story_new(&self, scene: &Scene, quest: u32) -> bool {
        let Some(q) = self
            .active_quests(scene)
            .into_iter()
            .find(|q| q.id == quest)
        else {
            return false;
        };
        !self.quest_ready(scene, q)
            && self.systems.as_ref().is_some_and(|p| {
                p.preferences
                    .get(&format!("QSTMSGNEW-{quest}"))
                    .is_none_or(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0))
            })
            && !keys(scene, quest).is_empty()
    }
    fn save_quest_story(&mut self, pending: Option<Pending>) -> Result<(), String> {
        let p = &mut self
            .systems
            .as_mut()
            .ok_or("Missing quest preferences")?
            .preferences;
        if let Some(v) = pending {
            p.insert(
                KEY.into(),
                serde_json::to_value(v).map_err(|e| e.to_string())?,
            );
        } else {
            p.remove(KEY);
        }
        Ok(())
    }
    pub fn base_tutorial_waits_for_quest(&self) -> bool {
        self.mission.is_none()
            && self.systems.as_ref().is_some_and(|p| {
                p.flag("Mission-5000")
                    && !p.flag("TutorialComplete")
                    && !p.flag("tutorialCompleted")
            })
    }
    pub fn quest_dialog_needs_commit(&self, scene: &Scene, quest: u32) -> bool {
        self.active_quests(scene).iter().any(|q| q.id == quest)
            && (self.quest_story_new(scene, quest) || self.base_tutorial_waits_for_quest())
    }
    pub fn begin_quest_story(
        &self,
        scene: &Scene,
        quest: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        if self.quest_story()?.is_some()
            || !self.quest_dialog_needs_commit(scene, quest)
            || self.mission.is_some() && !self.mission_paused()
        {
            return Err("Реплика задания недоступна".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        // SceneMain.questDisplayInfo0x9a561 finishes the still-active battle
        // tutorial on the first base quest tap, BEFORE showing its character text.
        // Returning from the battle alone does not set these completion flags.
        if self.base_tutorial_waits_for_quest() {
            let p = next.systems.as_mut().unwrap();
            p.set("TutorialComplete".into(), 1);
            p.set("tutorialCompleted".into(), 1);
            next.building_receipt(
                now,
                &format!("quest:{quest}"),
                quest,
                "tutorial_base_finish",
                BTreeMap::new(),
                vec![],
            )?;
        }
        if self.quest_story_new(scene, quest) {
            next.systems
                .as_mut()
                .unwrap()
                .set(format!("QSTMSGNEW-{quest}"), 0);
            next.save_quest_story(Some(Pending { quest, phrase: 0 }))?;
            next.building_receipt(
                now,
                &format!("quest:{quest}"),
                quest,
                "quest_story_open",
                BTreeMap::new(),
                vec![],
            )?;
        }
        next.validate(scene)?;
        Ok((next, "".into()))
    }
    pub fn advance_quest_story(&self, scene: &Scene, wall: i64) -> Result<(Self, String), String> {
        let mut pending = self.quest_story()?.ok_or("Нет реплики задания")?;
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        let quest = pending.quest;
        pending.phrase += 1;
        next.save_quest_story((pending.phrase < keys(scene, quest).len()).then_some(pending))?;
        next.building_receipt(
            now,
            &format!("quest:{quest}"),
            quest,
            "quest_story_next",
            BTreeMap::new(),
            vec![],
        )?;
        next.validate(scene)?;
        Ok((next, "".into()))
    }
    pub fn validate_quest_story(&self, scene: &Scene) -> Result<(), String> {
        let finishes = self
            .actions
            .iter()
            .filter(|a| a.kind == "tutorial_base_finish")
            .collect::<Vec<_>>();
        if finishes.len() > 1
            || finishes.iter().any(|a| {
                !a.gains.is_empty()
                    || !a.entropy.is_empty()
                    || !a.quest_events_pending.is_empty()
                    || a.source.is_some()
                    || a.shop.is_some()
                    || a.uid != format!("quest:{}", a.recipe)
                    || !scene
                        .progression
                        .as_ref()
                        .is_some_and(|p| p.quests.iter().any(|q| q.id == a.recipe))
                    || self.systems.as_ref().is_none_or(|p| {
                        !p.flag("Mission-5000")
                            || !p.flag("TutorialComplete")
                            || !p.flag("tutorialCompleted")
                    })
            })
        {
            return Err("Invalid base tutorial completion".into());
        }
        let mut expected = None::<Pending>;
        let mut seen = BTreeSet::new();
        for a in self
            .actions
            .iter()
            .filter(|a| a.kind.starts_with("quest_story_"))
        {
            if !a.gains.is_empty()
                || !a.entropy.is_empty()
                || !a.quest_events_pending.is_empty()
                || a.source.is_some()
                || a.shop.is_some()
                || a.uid != format!("quest:{}", a.recipe)
                || !scene
                    .progression
                    .as_ref()
                    .is_some_and(|p| p.quests.iter().any(|q| q.id == a.recipe))
                || keys(scene, a.recipe).is_empty()
            {
                return Err("Invalid quest dialogue receipt".into());
            }
            match a.kind.as_str() {
                "quest_story_open" => {
                    if expected.is_some()
                        || !seen.insert(a.recipe)
                        || self.systems.as_ref().is_none_or(|p| {
                            !p.preferences
                                .contains_key(&format!("QSTMSGNEW-{}", a.recipe))
                                || p.flag(&format!("QSTMSGNEW-{}", a.recipe))
                        })
                    {
                        return Err("Duplicate quest introduction".into());
                    }
                    expected = Some(Pending {
                        quest: a.recipe,
                        phrase: 0,
                    });
                }
                "quest_story_next" => {
                    let p = expected
                        .as_mut()
                        .filter(|p| p.quest == a.recipe)
                        .ok_or("Quest dialogue advance without introduction")?;
                    p.phrase += 1;
                    if p.phrase >= keys(scene, p.quest).len() {
                        expected = None;
                    }
                }
                _ => return Err("Unknown quest dialogue operation".into()),
            }
        }
        if self.quest_story()? != expected
            || expected.as_ref().is_some_and(|p| {
                self.mission.is_some() && !self.mission_paused()
                    || !self.active_quests(scene).iter().any(|q| q.id == p.quest)
            })
        {
            return Err("Quest dialogue continuation differs from journal".into());
        }
        Ok(())
    }
}
pub fn current(scene: &Scene, state: &State) -> Option<(String, String)> {
    if state.mission.is_some() && !state.mission_paused() {
        return None;
    }
    let p = state.quest_story().ok()??;
    let key = keys(scene, p.quest).get(p.phrase)?.clone();
    let text = scene
        .progression
        .as_ref()?
        .language
        .get(&key)?
        .replace("\\n", "\n");
    Some((key, text))
}
