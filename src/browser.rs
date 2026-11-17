//! Emscripten host hooks. The native build does not link JavaScript.
#[cfg(target_os = "emscripten")]
unsafe extern "C" {
    fn wz_sync_save() -> i32;
    fn wz_save_recovered();
    fn wz_random() -> u32;
    fn wz_next_frame();
    fn wz_viewport(ww: u32, wh: u32, rw: u32, rh: u32);
    fn wz_pointer_trace(down: u32, x: i32, y: i32, consumed: u32, panel: i32);
    fn wz_background_capture() -> i32;
    fn wz_display_density() -> f64;
    fn wz_canvas_size() -> u32;
}
#[cfg(target_os = "emscripten")]
pub fn drawing_size() -> (u32, u32) {
    let packed = unsafe { wz_canvas_size() };
    (packed >> 16, packed & 0xffff)
}
#[cfg(target_os = "emscripten")]
static DISPLAY_DENSITY: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(1f32.to_bits());

pub fn display_density() -> f64 {
    #[cfg(target_os = "emscripten")]
    return f32::from_bits(DISPLAY_DENSITY.load(std::sync::atomic::Ordering::Relaxed)) as f64;
    #[cfg(not(target_os = "emscripten"))]
    1.
}

/// Read layout once per frame, rather than crossing into JS for every HUD label.
pub fn update_display_density() {
    #[cfg(target_os = "emscripten")]
    {
        let density = unsafe { wz_display_density() };
        if density.is_finite() && density > 0. {
            DISPLAY_DENSITY.store(
                (density as f32).to_bits(),
                std::sync::atomic::Ordering::Relaxed,
            );
        }
    }
}

/// Save files retain device-independent zoom in the original 0.12..2 range;
/// drawing-buffer density must never be baked into camera state.
pub fn zoom_factor() -> f64 {
    display_density()
}
/// Diagnostic recording uses the real world renderer without interface layers.
/// It never changes the scene, simulation, profile or save admission.
pub fn background_capture() -> bool {
    #[cfg(target_os = "emscripten")]
    return unsafe { wz_background_capture() != 0 };
    #[cfg(not(target_os = "emscripten"))]
    false
}
#[cfg(target_os = "emscripten")]
pub fn trace_pointer(event: &sdl2::event::Event, consumed: bool, panel: i32) {
    use sdl2::event::Event;
    let (down, x, y) = match *event {
        Event::MouseButtonDown { x, y, .. } => (1, x, y),
        Event::MouseButtonUp { x, y, .. } => (0, x, y),
        _ => return,
    };
    unsafe {
        wz_pointer_trace(down, x, y, consumed.into(), panel);
    }
}
#[cfg(target_os = "emscripten")]
pub fn viewport(window: (u32, u32), output: (u32, u32)) {
    unsafe {
        wz_viewport(window.0, window.1, output.0, output.1);
    }
}

pub fn persist() -> Result<(), String> {
    #[cfg(target_os = "emscripten")]
    if unsafe { wz_sync_save() } != 0 {
        return Err("Browser save failed; export your save before closing this tab".into());
    }
    Ok(())
}

pub fn save_recovered() {
    #[cfg(target_os = "emscripten")]
    unsafe {
        wz_save_recovered();
    }
}

pub fn yield_frame(_duration: std::time::Duration) {
    #[cfg(target_os = "emscripten")]
    unsafe {
        wz_next_frame();
    }
    #[cfg(not(target_os = "emscripten"))]
    std::thread::sleep(_duration);
}

#[cfg(target_os = "emscripten")]
pub fn entropy() -> u32 {
    unsafe { wz_random() }
}

static POINTER_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
// SAFETY: This uniquely named bridge symbol is exported only by this crate.
#[unsafe(no_mangle)]
pub extern "C" fn wz_cancel_pointer() {
    POINTER_CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn take_pointer_cancel() -> bool {
    POINTER_CANCEL.swap(false, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_os = "emscripten")]
// SAFETY: This uniquely named bridge symbol is exported only by this crate.
#[unsafe(no_mangle)]
pub extern "C" fn wz_validate_import() -> i32 {
    let result = (|| -> Result<(), String> {
        let root = std::path::Path::new("/assets");
        let scene = crate::Scene::load(root)?;
        let raw = std::fs::read(root.join("scene.json")).map_err(|e| e.to_string())?;
        let bytes = std::fs::read("/tmp/import.json").map_err(|e| e.to_string())?;
        crate::save::validate_import(&bytes, &scene, &crate::save::digest(&raw))
    })();
    if result.is_ok() { 0 } else { -1 }
}
