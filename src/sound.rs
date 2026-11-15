//! Original sound catalog and a bounded software mixer. No game RNG is consumed.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub sha256: String,
    pub frames: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub volume: f32,
    pub polyphony: usize,
    #[serde(default)]
    pub cooldown_event: u32,
    #[serde(default)]
    pub cooldown_sound: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub path: String,
    pub channel: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub sample_rate: i32,
    pub clips: BTreeMap<String, Clip>,
    pub channels: BTreeMap<String, Channel>,
    pub events: BTreeMap<String, Vec<Event>>,
    #[serde(default)]
    pub localized_events: BTreeMap<u32, BTreeMap<String, Vec<Event>>>,
    pub units: BTreeMap<u32, String>,
    /// AudioManager.playTraktor uses this direct path, outside XML presets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tractor_path: Option<String>,
}
impl Config {
    pub fn events_for(&self, id: u32) -> &BTreeMap<String, Vec<Event>> {
        self.localized_events.get(&id).unwrap_or(&self.events)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.sample_rate != 44100
            || self
                .clips
                .values()
                .any(|c| c.frames == 0 || c.frames > 16 * 1024 * 1024)
            || self.clips.values().map(|c| c.frames).sum::<usize>() > 32 * 1024 * 1024
            || self.channels.values().any(|c| {
                !c.volume.is_finite()
                    || !(0. ..=1.).contains(&c.volume)
                    || c.polyphony == 0
                    || c.polyphony > 16
                    || c.cooldown_event > 3600
                    || c.cooldown_sound > 3600
            })
            || self
                .tractor_path
                .as_ref()
                .is_some_and(|p| !self.clips.contains_key(p))
            || self.localized_events.len() > 3
            || self
                .events
                .values()
                .chain(self.localized_events.values().flat_map(|v| v.values()))
                .any(|events| {
                    events.is_empty()
                        || events.iter().any(|e| {
                            !self.clips.contains_key(&e.path)
                                || !self.channels.contains_key(&e.channel)
                                || e.channel != events[0].channel
                        })
                })
        {
            return Err("Invalid bounded original audio catalog".into());
        }
        Ok(())
    }
}
struct Voice {
    data: Arc<[f32]>,
    position: usize,
    volume: f32,
}
struct Reservation {
    channel: String,
    end: u64,
}
pub struct Mixer {
    music: Option<Voice>,
    tractor: Option<Voice>,
    pub tractor_starts: u64,
    pub tractor_stops: u64,
    effects: Vec<Voice>,
    reservations: Vec<Reservation>,
    pub play_music: bool,
    pub play_sound: bool,
    pub frames: u64,
    pub nonzero_samples: u64,
    pub accepted_effects: u64,
    pub rejected_effects: u64,
    last_channel: BTreeMap<String, u64>,
    last_event: BTreeMap<String, u64>,
}
impl Default for Mixer {
    fn default() -> Self {
        Self {
            music: None,
            tractor: None,
            tractor_starts: 0,
            tractor_stops: 0,
            effects: vec![],
            reservations: vec![],
            play_music: true,
            play_sound: true,
            frames: 0,
            nonzero_samples: 0,
            accepted_effects: 0,
            rejected_effects: 0,
            last_channel: BTreeMap::new(),
            last_event: BTreeMap::new(),
        }
    }
}
impl Mixer {
    /// Web Audio owns playback on the audio thread. Advance admission/cooldown
    /// bookkeeping without mixing discarded PCM on the browser's UI thread.
    pub fn advance_playback_clock(&mut self, frames: u64) {
        let elapsed = frames.saturating_sub(self.frames);
        self.frames = self.frames.max(frames);
        for voice in &mut self.effects {
            voice.position = voice
                .position
                .saturating_add(usize::try_from(elapsed.saturating_mul(2)).unwrap_or(usize::MAX));
        }
        self.effects.retain(|v| v.position < v.data.len());
        self.reservations.retain(|r| r.end > self.frames);
    }
    pub fn clear_cache(&mut self) {
        self.effects.clear();
        self.music = None;
        self.stop_tractor();
    }
    /// SPALSoundChannel.play ignores repeated play on the same looping path.
    /// This direct channel has no preset reservation, cooldown or game RNG.
    pub fn start_tractor(&mut self, data: Arc<[f32]>) -> bool {
        if !self.play_sound || data.is_empty() {
            return false;
        }
        if self.tractor.is_none() {
            self.tractor = Some(Voice {
                data,
                position: 0,
                volume: 1.,
            });
            self.tractor_starts += 1;
        }
        true
    }
    pub fn stop_tractor(&mut self) {
        if self.tractor.take().is_some() {
            self.tractor_stops += 1;
        }
    }
    pub fn tractor_playing(&self) -> bool {
        self.tractor.is_some()
    }
    pub fn music(&mut self, data: Arc<[f32]>, volume: f32) {
        self.music = Some(Voice {
            data,
            position: 0,
            volume,
        });
    }
    /// Source AudioManager 0x132775: reject full channels, then check channel
    /// cooldown_sound and preset/event cooldown_event, before choosing a variant.
    pub fn allows(&self, event: &str, name: &str, channel: &Channel) -> bool {
        let elapsed = |last: Option<&u64>, seconds: u32| {
            last.is_none_or(|t| self.frames.saturating_sub(*t) / 44100 >= seconds as u64)
        };
        self.play_sound
            && self.reservations.len() < 32
            && self
                .reservations
                .iter()
                .filter(|v| v.channel == name && v.end > self.frames)
                .count()
                < channel.polyphony
            && elapsed(self.last_channel.get(name), channel.cooldown_sound)
            && elapsed(self.last_event.get(event), channel.cooldown_event)
    }
    pub fn effect_event(
        &mut self,
        data: Arc<[f32]>,
        event: &str,
        name: &str,
        channel: &Channel,
    ) -> bool {
        if data.is_empty() || !self.allows(event, name, channel) {
            self.rejected_effects += 1;
            return false;
        }
        // Source getCachedSoundChannel: uses one channel per path. Sparrow
        // SPALSoundChannel.play 0x21f18d ignores play while already playing;
        // AudioManager still reserves one slot until this call's full duration.
        self.reservations.push(Reservation {
            channel: name.into(),
            end: self.frames + (data.len() / 2) as u64,
        });
        if let Some(voice) = self
            .effects
            .iter_mut()
            .find(|v| Arc::ptr_eq(&v.data, &data))
        {
            voice.volume = channel.volume;
        } else {
            self.effects.push(Voice {
                data,
                position: 0,
                volume: channel.volume,
            });
        }
        self.last_channel.insert(name.into(), self.frames);
        self.last_event.insert(event.into(), self.frames);
        self.accepted_effects += 1;
        true
    }
    pub fn effect(&mut self, data: Arc<[f32]>, channel: &str, volume: f32, limit: usize) {
        self.effect_event(
            data,
            channel,
            channel,
            &Channel {
                volume,
                polyphony: limit,
                cooldown_event: 0,
                cooldown_sound: 0,
            },
        );
    }
    pub fn settings(&mut self, music: bool, sound: bool) {
        self.play_music = music;
        self.play_sound = sound;
        // onSettingsSetMusic: stops the theme. Enabling starts it from zero
        // through Audio.sync. onSettingsSetSound: gates new effects only.
        if !music {
            self.music = None;
        }
    }
    pub fn render(&mut self, output: &mut [f32]) {
        output.fill(0.);
        for sample in output.iter_mut() {
            let mut v = 0.;
            if self.play_music {
                if let Some(m) = &mut self.music {
                    if !m.data.is_empty() {
                        v += m.data[m.position % m.data.len()] * m.volume;
                        m.position += 1;
                    }
                }
            }
            if let Some(looping) = &mut self.tractor {
                v += looping.data[looping.position] * looping.volume;
                looping.position = (looping.position + 1) % looping.data.len();
            }
            {
                for e in &mut self.effects {
                    if let Some(s) = e.data.get(e.position) {
                        v += s * e.volume;
                        e.position += 1;
                    }
                }
            }
            *sample = v.clamp(-1., 1.);
            self.nonzero_samples += u64::from(*sample != 0.);
        }
        self.effects.retain(|e| e.position < e.data.len());
        self.frames += (output.len() / 2) as u64;
        self.reservations.retain(|r| r.end > self.frames);
    }
}
impl crate::production::State {
    pub fn audio_enabled(&self, key: &str) -> bool {
        self.systems
            .as_ref()
            .and_then(|s| s.preferences.get(key))
            .is_none_or(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(1) != 0))
    }
    /// Settings are preferences, like camera changes, not a resource/quest action.
    /// Caller publishes only after the same atomic Profile.commit as gameplay.
    pub fn toggle_audio(&self, scene: &crate::Scene, key: &str) -> Result<(Self, String), String> {
        if !["playMusic", "playSound"].contains(&key) {
            return Err("Unknown sound setting".into());
        }
        let mut next = self.clone();
        let enabled = !self.audio_enabled(key);
        next.systems
            .as_mut()
            .ok_or("Missing native preferences")?
            .preferences
            .insert(key.into(), enabled.into());
        next.validate(scene)?;
        Ok((
            next,
            format!(
                "{}: {}",
                if key == "playMusic" {
                    "Музыка"
                } else {
                    "Звуки"
                },
                if enabled {
                    "включено"
                } else {
                    "выключено"
                }
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn web_audio_clock_releases_finished_channels_without_replaying_samples() {
        let mut mixer = Mixer::default();
        let channel = Channel {
            volume: 1.,
            polyphony: 1,
            cooldown_event: 1,
            cooldown_sound: 0,
        };
        let clip: Arc<[f32]> = vec![0.25; 882].into();
        assert!(mixer.effect_event(clip.clone(), "click", "ui", &channel));
        mixer.advance_playback_clock(441);
        assert!(mixer.effects.is_empty());
        assert!(!mixer.allows("click", "ui", &channel));
        mixer.advance_playback_clock(44100);
        assert!(mixer.allows("click", "ui", &channel));
        assert!(mixer.effect_event(clip, "click", "ui", &channel));
        mixer.advance_playback_clock(0);
        assert_eq!(mixer.frames, 44100);
        assert_eq!(mixer.accepted_effects, 2);
    }
    #[test]
    fn real_callback_core_loops_music_limits_channels_clamps_and_mutes_without_replay() {
        let mut m = Mixer::default();
        m.music(Arc::from([0.5, -0.5]), 0.3);
        m.effect(Arc::from([1., 1., 1., 1.]), "ui", 1., 1);
        m.effect(Arc::from([1., 1.]), "ui", 1., 1);
        let mut out = [0.; 8];
        m.render(&mut out);
        assert_eq!(&out[..4], &[1., 0.85, 1., 0.85]);
        assert_eq!(m.effects.len(), 0);
        m.settings(false, false);
        m.render(&mut out);
        assert_eq!(out, [0.; 8]);
        m.effect(Arc::from([1., 1.]), "ui", 1., 5);
        assert!(m.effects.is_empty());
        m.settings(true, true);
        m.music(Arc::from([0.5, -0.5]), 0.3);
        m.render(&mut out);
        assert_eq!(&out[..2], &[0.15, -0.15]);
        assert!(m.nonzero_samples > 0);
    }
}
