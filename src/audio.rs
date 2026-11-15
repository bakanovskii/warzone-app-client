//! SDL output is optional: no audio device must never prevent loading a profile.
#[cfg(target_os = "emscripten")]
use crate::browser_audio::{Device as AudioDevice, Output};
#[cfg(not(target_os = "emscripten"))]
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use warzone_app_client::{Scene, contained_file, production::State, sound::Config};
#[cfg(not(target_os = "emscripten"))]
struct Output(warzone_app_client::sound::Mixer);
#[cfg(not(target_os = "emscripten"))]
impl AudioCallback for Output {
    type Channel = f32;
    fn callback(&mut self, out: &mut [f32]) {
        self.0.render(out);
    }
}
pub struct Audio {
    device: AudioDevice<Output>,
    config: Config,
    clips: BTreeMap<String, Arc<[f32]>>,
    pack: PathBuf,
    clip_order: BTreeMap<String, u64>,
    clip_serial: u64,
    decoded_bytes: usize,
    theme: String,
    language: u32,
    revision: u64,
    events: BTreeMap<String, u64>,
    pending_result: Option<(String, &'static str)>,
}
impl Audio {
    pub fn open(_sdl: &sdl2::Sdl, scene: &Scene, pack: &Path) -> Result<Option<Self>, String> {
        let Some(config) = &scene.audio else {
            return Ok(None);
        };
        #[cfg(not(target_os = "emscripten"))]
        let device = {
            let audio = _sdl.audio()?;
            let spec = AudioSpecDesired {
                freq: Some(44100),
                channels: Some(2),
                samples: Some(1024),
            };
            let device = audio.open_playback(None, &spec, |actual| {
                assert_eq!(actual.freq, 44100);
                assert_eq!(actual.channels, 2);
                Output(warzone_app_client::sound::Mixer::default())
            })?;
            device.resume();
            device
        };
        #[cfg(target_os = "emscripten")]
        let device = AudioDevice::new();
        Ok(Some(Self {
            device,
            config: config.clone(),
            clips: BTreeMap::new(),
            pack: pack.into(),
            clip_order: BTreeMap::new(),
            clip_serial: 0,
            decoded_bytes: 0,
            theme: String::new(),
            language: 1,
            revision: u64::MAX,
            events: BTreeMap::new(),
            pending_result: None,
        }))
    }
    fn load_clip(&mut self, name: &str) -> Option<Arc<[f32]>> {
        self.clip_serial += 1;
        self.clip_order.insert(name.into(), self.clip_serial);
        if let Some(data) = self.clips.get(name) {
            return Some(data.clone());
        }
        let clip = self.config.clips.get(name)?;
        let path = contained_file(&self.pack, name).ok()?;
        let raw = std::fs::read(path).ok()?;
        if raw.len() != clip.frames * 4 || warzone_app_client::save::digest(&raw) != clip.sha256 {
            return None;
        }
        let data: Arc<[f32]> = raw
            .chunks_exact(2)
            .map(|p| i16::from_le_bytes([p[0], p[1]]) as f32 / 32768.)
            .collect::<Vec<_>>()
            .into();
        self.decoded_bytes += data.len() * 4;
        self.clips.insert(name.into(), data.clone());
        while self.decoded_bytes > 96 * 1024 * 1024 && self.clips.len() > 1 {
            let oldest = self
                .clips
                .keys()
                .filter(|k| k.as_str() != name)
                .min_by_key(|k| self.clip_order.get(*k))
                .cloned()?;
            self.decoded_bytes -= self.clips.remove(&oldest)?.len() * 4;
            self.clip_order.remove(&oldest);
        }
        Some(data)
    }
    pub fn event(&mut self, event: &str) {
        let Some(choices) = self.config.events_for(self.language).get(event).cloned() else {
            return;
        };
        *self.events.entry(event.into()).or_default() += 1;
        let Some(first) = choices.first() else { return };
        let channel = self.config.channels[&first.channel].clone();
        {
            let mut mixer = self.device.lock();
            if !mixer.0.allows(event, &first.channel, &channel) {
                mixer.0.rejected_effects += 1;
                return;
            }
        }
        let Ok(random) = warzone_app_client::save::entropy() else {
            return;
        };
        let e = &choices[random as usize % choices.len()];
        if let Some(data) = self.load_clip(&e.path) {
            self.device
                .lock()
                .0
                .effect_event(data, event, &e.channel, &channel);
        }
    }
    pub fn sync(&mut self, state: &State) {
        self.sync_presentation(state, true);
    }
    /// A durable result can precede the final shot/death animation. Admit its
    /// fanfare only when the result presentation is ready, once per live result.
    /// Opening an already completed save remains silent.
    pub fn sync_presentation(&mut self, state: &State, result_ready: bool) {
        let language_changed = self.language != state.language_id();
        if language_changed {
            self.language = state.language_id();
            self.device.lock().0.clear_cache();
        }
        let enabled = state.audio_enabled("playMusic");
        let music_changed = self.device.lock().0.play_music != enabled;
        self.device
            .lock()
            .0
            .settings(enabled, state.audio_enabled("playSound"));
        let theme = if state.mission.is_some() && !state.mission_paused() {
            "music/battle_theme"
        } else {
            "music/main_theme"
        };
        if self.theme != theme || music_changed || language_changed {
            if let Some(e) = self
                .config
                .events
                .get(theme)
                .filter(|_| enabled)
                .and_then(|v| v.first())
                .cloned()
            {
                if let Some(data) = self.load_clip(&e.path) {
                    self.device
                        .lock()
                        .0
                        .music(data, self.config.channels[&e.channel].volume);
                }
            }
            self.theme = theme.into();
        }
        if self.revision != state.revision {
            if self.revision != u64::MAX {
                let previous = self.revision;
                for a in state.actions.iter().filter(|a| a.sequence > previous) {
                    if a.kind == "obstacle_clear" && [8009, 8022, 8023].contains(&a.recipe) {
                        if let Some(path) = self.config.tractor_path.clone() {
                            if let Some(data) = self.load_clip(&path) {
                                self.device.lock().0.start_tractor(data);
                            }
                        }
                    }
                    // Source delete animation completion stops the shared
                    // direct channel, including after a tree's one-shot sound.
                    if a.kind == "obstacle_bonus" {
                        self.device.lock().0.stop_tractor();
                    }
                    let event = match a.kind.as_str() {
                        // SceneBattle.putUnit plays at accepted placement, before Start.
                        "mission_deploy" => None,
                        "mission_win" | "mission_defeat" => {
                            self.pending_result = Some((
                                a.uid.clone(),
                                if a.kind == "mission_win" {
                                    "ui/mission_win"
                                } else {
                                    "ui/mission_lose"
                                },
                            ));
                            None
                        }
                        // SceneMain.questDisplayCompleteDialog plays this on
                        // condition completion, not again on reward collection.
                        "quest_claim" => None,
                        "start" | "workshop_start" | "building_buy" | "shop_buy"
                        | "workshop_unlock" | "resource_speedup" | "workshop_speedup"
                        | "research_speedup" | "building_speedup" | "bank_free" => {
                            Some("ui/buy_object")
                        }
                        "land_buy_cash" | "land_buy_coins" | "land_buy_shortfall" => {
                            Some("ui/buy_object")
                        }
                        "obstacle_clear" if ![8009, 8022, 8023].contains(&a.recipe) => {
                            Some("ui/tree_tehnika")
                        }
                        "obstacle_bonus" => Some("ui/vipadanie_bonusov"),
                        // FallingItem owns one drop sound per actual icon at
                        // its staggered creation time, rather than per receipt.
                        "flight_upgrade"
                        | "flight_upgrade_touch"
                        | "flight_research"
                        | "flight_research_touch" => Some("ui/get_item"),
                        "building_finish" => Some("ui/construct"),
                        _ => None,
                    };
                    if let Some(e) = event {
                        self.event(e);
                    }
                }
            }
            self.revision = state.revision;
        }
        if let Some((id, event)) = self.pending_result.clone() {
            if !state
                .mission
                .as_ref()
                .is_some_and(|m| m.id == id && m.battle.result.is_some())
            {
                self.pending_result = None;
            } else if result_ready {
                self.pending_result = None;
                self.event(event);
            }
        }
        if state.mission.is_some() && !state.mission_paused() {
            self.device.lock().0.stop_tractor();
        }
    }
    pub fn unit_event(&mut self, id: u32, event: &str) {
        if let Some(preset) = self.config.units.get(&id) {
            self.event(&format!("{preset}/{event}"));
        }
    }
    pub fn report(&mut self) -> serde_json::Value {
        let m = self.device.lock();
        serde_json::json!({"available":true,"theme":self.theme,"frames":m.0.frames,"nonzero_samples":m.0.nonzero_samples,"accepted_effects":m.0.accepted_effects,"rejected_effects":m.0.rejected_effects,"playMusic":m.0.play_music,"playSound":m.0.play_sound,"clips":self.clips.len(),"events":self.events,"language":self.language,"tractor_playing":m.0.tractor_playing(),"tractor_starts":m.0.tractor_starts,"tractor_stops":m.0.tractor_stops})
    }
}
