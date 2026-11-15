// BufferSource playback runs on Web Audio's rendering thread. UI stalls can
// delay a new sound, but cannot loop a click or corrupt a ScriptProcessor block.
window.WarzoneAudio = (() => {
  let context,
    musicGain,
    soundGain,
    bytes = 0,
    serial = 0;
  const cache = new Map(),
    voices = new Map();
  const stats = { started: 0, duplicateStartsIgnored: 0, cacheBytes: 0, plays: [] };
  const init = () => {
    if (!context) {
      const Audio = window.AudioContext || window.webkitAudioContext;
      if (!Audio) return;
      try {
        context = new Audio({ sampleRate: 44100 });
      } catch (error) {
        console.warn("Web Audio unavailable:", error);
        return;
      }
      musicGain = context.createGain();
      soundGain = context.createGain();
      musicGain.connect(context.destination);
      soundGain.connect(context.destination);
    }
    context.resume().catch(console.error);
  };
  const stop = (kind) => {
    for (const [key, voice] of voices) {
      if (kind === 3 || voice.kind === kind) {
        voice.source.onended = null;
        voice.source.stop();
        voice.source.disconnect();
        voice.gain.disconnect();
        voices.delete(key);
      }
    }
  };
  const play = (id, pointer, length, volume, kind, heap) => {
    if (!context) return;
    const key = kind === 0 ? `effect:${id}` : `loop:${kind}`;
    const previous = voices.get(key);
    if (previous && kind === 0 && context.currentTime < previous.endsAt) {
      previous.gain.gain.value = volume;
      stats.duplicateStartsIgnored++;
      return;
    }
    if (kind !== 0) stop(kind);
    let clip = cache.get(id);
    if (!clip) {
      const frames = length / 2,
        buffer = context.createBuffer(2, frames, 44100);
      const left = buffer.getChannelData(0),
        right = buffer.getChannelData(1);
      for (let i = 0, offset = pointer / 4; i < frames; i++) {
        left[i] = heap[offset + i * 2];
        right[i] = heap[offset + i * 2 + 1];
      }
      clip = { buffer, bytes: length * 4, used: ++serial };
      cache.set(id, clip);
      bytes += clip.bytes;
    }
    clip.used = ++serial;
    const source = context.createBufferSource(),
      gain = context.createGain();
    source.buffer = clip.buffer;
    source.loop = kind !== 0;
    gain.gain.value = volume;
    source.connect(gain);
    gain.connect(kind === 1 ? musicGain : soundGain);
    const voice = { source, gain, kind, endsAt: context.currentTime + clip.buffer.duration };
    voices.set(key, voice);
    source.onended = () => {
      if (voices.get(key) === voice) voices.delete(key);
      source.disconnect();
      gain.disconnect();
    };
    source.start();
    stats.started++;
    stats.plays.push({ id, kind, loop: source.loop, duration: clip.buffer.duration });
    if (stats.plays.length > 32) stats.plays.shift();
    while (bytes > 96 * 1024 * 1024 && cache.size > 1) {
      const oldest = [...cache]
        .filter(([key]) => key !== id)
        .sort((a, b) => a[1].used - b[1].used)[0];
      bytes -= oldest[1].bytes;
      cache.delete(oldest[0]);
    }
    stats.cacheBytes = bytes;
  };
  return {
    init,
    play,
    stop,
    time: () => context?.currentTime || 0,
    stats,
    settings(music, sound) {
      if (!context) return;
      musicGain.gain.value = music ? 1 : 0;
      soundGain.gain.value = sound ? 1 : 0;
      if (!sound) {
        stop(0);
        stop(2);
      }
    },
  };
})();
