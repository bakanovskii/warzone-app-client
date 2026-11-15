//! Original localized phrase masks and PersonagesQueue character selection.
use crate::{Scene, production::State};
pub fn phrase_keys(scene: &Scene, mask: &str) -> Vec<String> {
    let mut keys = legacy_phrase_keys(scene, mask);
    let mut seen = std::collections::BTreeSet::new();
    keys.retain(|k| seen.insert(k.to_ascii_lowercase()));
    keys
}
/// Older saved mission conversations used the expanded locale alias table.
/// Retain their sequence until dismissal; newly queued messages are canonical.
pub fn message_keys(scene: &Scene, message: &crate::battle::StoryMessage) -> Vec<String> {
    if message.canonical_phrases {
        phrase_keys(scene, &message.mask)
    } else {
        legacy_phrase_keys(scene, &message.mask)
    }
}
fn legacy_phrase_keys(scene: &Scene, mask: &str) -> Vec<String> {
    let mask_lower = mask.to_lowercase();
    let Some(language) = scene.progression.as_ref().map(|p| &p.language) else {
        return vec![mask.into()];
    };
    let mut keys: Vec<_> = language
        .keys()
        .filter(|k| k.len() > mask_lower.len() && k.to_lowercase().starts_with(&mask_lower))
        .cloned()
        .collect();
    // Localizations0x23fb5 sorts by NSString.intValue after mask + separator.
    keys.sort_by_key(|key| {
        key.get(mask.len() + 1..)
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<usize>()
            .unwrap_or(0)
    });
    if keys.is_empty() {
        keys.push(mask.into());
    }
    keys
}
pub fn current(scene: &Scene, state: &State) -> Option<(String, String)> {
    if let Some(message) = crate::quest_story::current(scene, state) {
        return Some(message);
    }
    if let Some(key) = state
        .mission
        .as_ref()
        .and_then(|s| s.battle.pvp_message(scene.combat.as_ref()?))
    {
        return Some((
            key.into(),
            scene
                .progression
                .as_ref()?
                .language
                .get(key)
                .cloned()
                .unwrap_or_else(|| key.into())
                .replace("\\n", "\n"),
        ));
    }
    if let Some(step) = state
        .mission
        .as_ref()
        .and_then(|s| s.battle.tutorial_step(scene.combat.as_ref()?))
    {
        if step.text.is_empty() || step.id == 8 && state.mission.as_ref()?.battle.result.is_some() {
            return None;
        }
        let text = scene
            .progression
            .as_ref()?
            .language
            .get(&step.text)
            .cloned()
            .unwrap_or_else(|| step.text.clone())
            .replace("\\n", "\n");
        return Some((step.text.clone(), text));
    }
    let m = state
        .mission
        .as_ref()?
        .battle
        .story
        .as_ref()?
        .message
        .as_ref()?;
    let key = message_keys(scene, m).get(m.phrase)?.clone();
    let text = scene
        .progression
        .as_ref()?
        .language
        .get(&key)
        .cloned()
        .unwrap_or_else(|| key.clone())
        .replace("\\n", "\n");
    Some((key, text))
}
pub fn character(key: &str) -> (usize, &'static str, bool) {
    let (index, name) = [
        ("alice_happy", 2, "alisa_happy"),
        ("alice_think", 1, "alisa_thinking"),
        ("alice_rage", 3, "alisa_angry"),
        ("alice", 0, "alisa_default"),
        ("stein_scared", 7, "stain_scared"),
        ("stein_rage", 6, "stain_angry"),
        ("stein", 5, "stain"),
        ("skalov_rage", 12, "skalov_angry"),
        ("skalov_happy", 11, "skalov_happy"),
        ("skalov", 10, "skalov"),
        ("nitro_think", 16, "nitro_thinking"),
        ("nitro", 15, "nitro_happy"),
        ("lisa_sad", 21, "medic_sad"),
        ("lisa_rage", 22, "medic_sad"),
        ("lisa", 20, "medic_default"),
    ]
    .into_iter()
    .find(|(tag, _, _)| key.contains(tag))
    .map(|(_, index, name)| (index, name))
    .unwrap_or((0, "alisa_default"));
    (index, name, key.contains("_right"))
}
/// Phone offsets in PersonageDialog.getPersonageHeadShift/FlipShift/BalloonShift.
pub fn offsets(index: usize, right: bool) -> (f64, f64, f64) {
    let head = match index {
        4 => -50.,
        5..=7 => -25.,
        10..=12 => -12.,
        15..=16 => -27.,
        21..=22 => -5.,
        _ => 0.,
    };
    let flip = match index {
        0..=7 => 40.,
        10..=12 | 20..=22 => 30.,
        15..=16 => 35.,
        _ => 0.,
    };
    let balloon = match (index, right) {
        (0..=3, true) => -10.,
        (5, true) => -20.,
        (6, true) => -30.,
        (7, true) => -20.,
        (10..=12, true) => -40.,
        (10..=12, false) => 20.,
        (15..=16, true) => -5.,
        _ => 0.,
    };
    (head, flip, balloon)
}
