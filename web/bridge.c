#include <emscripten.h>

// Asyncify yields to the browser while IndexedDB commits BOTH state and backup.
// Rust publishes the candidate only after this returns success.
EM_ASYNC_JS(int, wz_sync_save, (), {
    try {
        await new Promise((resolve, reject) => FS.syncfs(false, error => error ? reject(error) : resolve()));
        window.dispatchEvent(new Event('warzone-saved'));
        return 0;
    } catch (error) {
        console.error('Warzone save:', error);
        return -1;
    }
});

EM_JS(unsigned, wz_random, (), {
    return crypto.getRandomValues(new Uint32Array(1))[0];
});

// SDL_RenderPresent cannot wait for browser vsync. Yield once per real display
// frame so WebGL command buffers cannot grow faster than the GPU consumes them.
EM_ASYNC_JS(void, wz_next_frame, (), {
    const now=performance.now();
    if(Module.warzoneFrameStart){
        const times=Module.warzoneFrameTimes ||= [];
        times.push(now-Module.warzoneFrameStart);
        if(times.length>180)times.shift();
    }
    if (!Module.warzoneFirstFrame) {
        Module.warzoneFirstFrame = true;
        window.dispatchEvent(new Event('warzone-frame'));
    }
    await new Promise(resolve => requestAnimationFrame(resolve));
    Module.warzoneFrameStart=performance.now();
});

EM_JS(double, wz_audio_clock, (), { return window.WarzoneAudio?.time() || 0; });
EM_JS(int, wz_background_capture, (), { return Module.warzoneBackgroundCapture ? 1 : 0; });
EM_JS(double, wz_display_density, (), {
    return Math.min(Module.canvas.width / Math.max(1, Module.canvas.clientWidth),
                    Module.canvas.height / Math.max(1, Module.canvas.clientHeight));
});
// Each dimension is below 65536 (the shell caps the buffer at 3840x2160).
EM_JS(unsigned, wz_canvas_size, (), {
    return (Module.canvas.width << 16) | Module.canvas.height;
});
EM_JS(void, wz_viewport, (unsigned ww, unsigned wh, unsigned rw, unsigned rh), {
    Module.warzoneViewport={window:[ww,wh],output:[rw,rh]};
});
EM_JS(void, wz_pointer_trace, (unsigned down, int x, int y, unsigned consumed, int panel), {
    if(Module.warzoneTrace){
        Module.warzoneTrace.push({down,x,y,consumed,panel});
        if(Module.warzoneTrace.length>32)Module.warzoneTrace.shift();
    }
});
EM_JS(void, wz_audio_play, (unsigned id, const float *data, unsigned len, float volume, unsigned kind), {
    window.WarzoneAudio?.play(id, data, len, volume, kind, HEAPF32);
});
EM_JS(void, wz_audio_settings, (unsigned music, unsigned sound), { window.WarzoneAudio?.settings(!!music, !!sound); });
EM_JS(void, wz_audio_stop, (unsigned kind), { window.WarzoneAudio?.stop(kind); });
