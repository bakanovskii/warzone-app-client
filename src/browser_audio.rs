//! Browser playback uses AudioBufferSourceNodes, not SDL's main-thread
//! ScriptProcessor callback. A slow save/draw cannot repeat its last PCM block.
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::{Arc, Weak},
};
use warzone_app_client::sound::{Channel, Mixer};

unsafe extern "C" {
    fn wz_audio_clock() -> f64;
    fn wz_audio_play(id: u32, data: *const f32, len: usize, volume: f32, kind: u32);
    fn wz_audio_settings(music: u32, sound: u32);
    fn wz_audio_stop(kind: u32);
}
pub struct Device<T> {
    output: T,
}
pub struct Output(pub Playback);
impl Device<Output> {
    pub fn new() -> Self {
        Self {
            output: Output(Playback {
                mixer: Mixer::default(),
                ids: BTreeMap::new(),
                serial: 0,
            }),
        }
    }
    pub fn lock(&mut self) -> &mut Output {
        self.output
            .0
            .mixer
            .advance_playback_clock((unsafe { wz_audio_clock() } * 44100.) as u64);
        &mut self.output
    }
}
pub struct Playback {
    mixer: Mixer,
    ids: BTreeMap<usize, (Weak<[f32]>, u32)>,
    serial: u32,
}
impl Deref for Playback {
    type Target = Mixer;
    fn deref(&self) -> &Mixer {
        &self.mixer
    }
}
impl DerefMut for Playback {
    fn deref_mut(&mut self) -> &mut Mixer {
        &mut self.mixer
    }
}
impl Playback {
    fn play(&mut self, data: &Arc<[f32]>, volume: f32, kind: u32) {
        let pointer = data.as_ptr() as usize;
        let id = self
            .ids
            .get(&pointer)
            .and_then(|(weak, id)| weak.upgrade().filter(|v| Arc::ptr_eq(v, data)).map(|_| *id));
        let id = id.unwrap_or_else(|| {
            self.serial += 1;
            self.ids
                .insert(pointer, (Arc::downgrade(data), self.serial));
            self.serial
        });
        unsafe {
            wz_audio_play(id, data.as_ptr(), data.len(), volume, kind);
        }
    }
    pub fn effect_event(
        &mut self,
        data: Arc<[f32]>,
        event: &str,
        name: &str,
        channel: &Channel,
    ) -> bool {
        let accepted = self.mixer.effect_event(data.clone(), event, name, channel);
        if accepted {
            self.play(&data, channel.volume, 0);
        }
        accepted
    }
    pub fn music(&mut self, data: Arc<[f32]>, volume: f32) {
        self.play(&data, volume, 1);
        self.mixer.music(data, volume);
    }
    pub fn start_tractor(&mut self, data: Arc<[f32]>) -> bool {
        let playing = self.mixer.tractor_playing();
        let accepted = self.mixer.start_tractor(data.clone());
        if accepted && !playing {
            self.play(&data, 1., 2);
        }
        accepted
    }
    pub fn stop_tractor(&mut self) {
        if self.mixer.tractor_playing() {
            unsafe {
                wz_audio_stop(2);
            }
        }
        self.mixer.stop_tractor();
    }
    pub fn settings(&mut self, music: bool, sound: bool) {
        if self.mixer.play_music != music || self.mixer.play_sound != sound {
            unsafe {
                wz_audio_settings(music.into(), sound.into());
            }
        }
        self.mixer.settings(music, sound);
    }
    pub fn clear_cache(&mut self) {
        unsafe {
            wz_audio_stop(3);
        }
        self.mixer.clear_cache();
    }
}
