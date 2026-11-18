use rusttype::{Font, Scale, point};
use sdl2::{
    pixels::{Color, PixelFormatEnum},
    rect::{FPoint, FRect, Point, Rect},
    render::{Canvas, Texture, TextureCreator, Vertex},
    video::{Window, WindowContext},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};
use warzone_app_client::{Camera, Entity, Scene, contained_file};
fn source_quad(
    canvas: &mut Canvas<Window>,
    points: [[f64; 2]; 4],
    color: Color,
) -> Result<(), String> {
    let vertices = points.map(|p| Vertex {
        position: FPoint::new(p[0] as f32, p[1] as f32),
        color,
        tex_coord: FPoint::new(0., 0.),
    });
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    canvas.render_geometry(&vertices, None, &[0, 1, 2, 0, 2, 3])
}
fn source_attack_area(
    canvas: &mut Canvas<Window>,
    points: &[[f64; 2]],
    zoom: f64,
) -> Result<(), String> {
    // Draw.drawDottedLine0xeacd9: six world X units on, six off, thickness3.
    for pair in points.windows(2) {
        let (mut a, mut b) = (pair[0], pair[1]);
        if a[0] > b[0] {
            std::mem::swap(&mut a, &mut b)
        }
        let dx = b[0] - a[0];
        if dx <= 0. {
            continue;
        }
        let dy = b[1] - a[1];
        let mut x = 0.;
        while x < dx {
            let e = (x + 6. * zoom).min(dx);
            source_segment(
                canvas,
                [a[0] + x, a[1] + dy * x / dx],
                [a[0] + e, a[1] + dy * e / dx],
                3. * zoom,
                Color::RGB(255, 0, 0),
            )?;
            x += 12. * zoom;
        }
    }
    Ok(())
}
fn source_segment(
    canvas: &mut Canvas<Window>,
    a: [f64; 2],
    b: [f64; 2],
    width: f64,
    color: Color,
) -> Result<(), String> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    if length < 0.001 {
        return Ok(());
    }
    let n = [-dy / length * width / 2., dx / length * width / 2.];
    let vertices = [
        [a[0] + n[0], a[1] + n[1]],
        [b[0] + n[0], b[1] + n[1]],
        [b[0] - n[0], b[1] - n[1]],
        [a[0] - n[0], a[1] - n[1]],
    ]
    .map(|p| Vertex {
        position: FPoint::new(p[0] as f32, p[1] as f32),
        color,
        tex_coord: FPoint::new(0., 0.),
    });
    canvas.render_geometry(&vertices, None, &[0, 1, 2, 0, 2, 3])
}

// SDL's Rust BlendMode enum contains only the standard modes. Declare the two
// C entry points with their unsigned ABI representation so a composed numeric
// mode never becomes an invalid Rust enum discriminant. OpenGL supports this
// blend; software renderers may reject it and retain straight-alpha fallback.
fn source_premultiplied_blend(texture: &mut Texture<'_>) -> bool {
    unsafe extern "C" {
        #[link_name = "SDL_ComposeCustomBlendMode"]
        fn compose(sc: u32, dc: u32, co: u32, sa: u32, da: u32, ao: u32) -> u32;
        #[link_name = "SDL_SetTextureBlendMode"]
        fn set(texture: *mut sdl2::sys::SDL_Texture, mode: u32) -> i32;
    }
    // ONE, ONE_MINUS_SRC_ALPHA, ADD for both source color and alpha.
    unsafe {
        let mode = compose(2, 6, 1, 2, 6, 1);
        set(texture.raw(), mode) == 0
    }
}

// PKUtils.convertToGrayscale0x1dc11 draws into a premultiplied32Little/
// premultipliedLast bitmap, then applies double0.30R+0.59G+0.11B with truncation.
// Transform the premultiplied bytes; alpha and the unfiltered atlas stay intact.
// GraphicsManager.copyImage:withBrightness: (0xd3279), bkg down image +20%.
fn source_highlight_pixels(rgba: &mut [u8]) {
    for p in rgba.chunks_exact_mut(4) {
        for v in &mut p[..3] {
            *v = (*v as f32 + *v as f32 * 0.2).round().clamp(0., 255.) as u8;
        }
    }
}
fn source_image_pixels(rgba: &mut [u8], grayscale: bool, premultiplied_output: bool) {
    if grayscale {
        for p in rgba.chunks_exact_mut(4) {
            let a = p[3] as u16;
            let r = (p[0] as u16 * a + 127) / 255;
            let g = (p[1] as u16 * a + 127) / 255;
            let b = (p[2] as u16 * a + 127) / 255;
            let y = (r as f64 * 0.30 + g as f64 * 0.59 + b as f64 * 0.11) as u16;
            let y = if premultiplied_output {
                y
            } else {
                (y * 255 + a / 2).checked_div(a).unwrap_or(0)
            };
            p[..3].fill(y.min(255) as u8);
        }
    } else if premultiplied_output {
        for p in rgba.chunks_exact_mut(4) {
            let a = p[3] as u16;
            for c in &mut p[..3] {
                *c = ((*c as u16 * a + 127) / 255) as u8;
            }
        }
    }
}

// SPTexture.initWithWidth:height:scale:colorSpace:draw:0x2156d9 packs the
// premultiplied bitmap's upper channel nibbles (0x215836..854), then uploads
// SPGLTexture format9 = GL_UNSIGNED_SHORT_4_4_4_4. Keep UIKit images RGBA8888.
fn source_sparrow_pixels(rgba: &mut [u8], grayscale: bool, premultiplied_output: bool) {
    for p in rgba.chunks_exact_mut(4) {
        // CGContext's image draw truncates these premultiplied channels.
        // Rounding here crosses a nibble boundary (238*17/255=15.86),
        // producing a visible17-level error after RGBA4444 unpacking.
        let a = p[3] as u16;
        for c in &mut p[..3] {
            *c = (*c as u16 * a / 255) as u8;
        }
        if grayscale {
            let y = (p[0] as f64 * 0.30 + p[1] as f64 * 0.59 + p[2] as f64 * 0.11) as u8;
            p[..3].fill(y);
        }
        for byte in p.iter_mut() {
            *byte = (*byte >> 4) * 17;
        }
        if !premultiplied_output {
            let a = p[3] as u16;
            for c in &mut p[..3] {
                *c = (*c as u16 * 255 + a / 2)
                    .checked_div(a)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod source_texture_precision_checks {
    use super::*;
    #[test]
    fn source_premultiplies_before_packing_nibbles_and_preserves_uikit() {
        let source = [90, 150, 230, 100, 255, 255, 255, 0, 70, 141, 250, 255];
        let mut gl = source;
        source_sparrow_pixels(&mut gl, false, true);
        assert_eq!(gl, [34, 51, 85, 102, 0, 0, 0, 0, 68, 136, 255, 255]);
        let mut ui = source;
        source_image_pixels(&mut ui, false, true);
        assert_eq!(&ui[..4], &[35, 59, 90, 100]);
        let mut boundary = [238, 238, 238, 17];
        source_sparrow_pixels(&mut boundary, false, true);
        assert_eq!(boundary, [0, 0, 0, 17]);
        let mut fallback = source;
        source_sparrow_pixels(&mut fallback, false, false);
        assert_eq!(fallback[3], 102);
        assert_eq!(&fallback[4..8], &[0, 0, 0, 0]);
        for i in [0, 1, 2, 8, 9, 10] {
            let alpha = fallback[i / 4 * 4 + 3] as u16;
            assert!(((fallback[i] as u16 * alpha + 127) / 255).abs_diff(gl[i] as u16) <= 1);
        }
    }
}

// CoreText's point size scales the EM square. rusttype::Scale::uniform instead
// specifies ascent-minus-descent pixel height, which made every retained source
// label too small despite loading the correct original font file.
fn source_text_scale(font: &Font<'_>, points: f32) -> Scale {
    let metrics = font.v_metrics_unscaled();
    Scale::uniform(points * (metrics.ascent - metrics.descent) / font.units_per_em() as f32)
}
fn source_label_pad(
    size: f32,
    face: usize,
    style: Option<&warzone_app_client::interface::Node>,
) -> f32 {
    let effect = style.map_or(0., |n| {
        n.shadow.as_ref().map_or(0., |e| {
            e.blur_x.abs() * 2. + e.offset_x.abs().max(e.offset_y.abs())
        }) + if n.glow.is_some() {
            size as f64 * 0.05
        } else {
            0.
        }
    });
    if effect > 0. {
        (effect.ceil() + 2.) as f32
    } else if face == TITLE {
        4.
    } else {
        1.
    }
}
fn source_advance(font: &Font<'_>, text: &str, size: f32, spacing: f64) -> f64 {
    let glyphs: Vec<_> = font
        .layout(text, source_text_scale(font, size), point(0., 0.))
        .collect();
    glyphs.last().map_or(0., |g| {
        g.position().x as f64 + g.unpositioned().h_metrics().advance_width as f64
    }) + text.chars().count().saturating_sub(1) as f64 * spacing
}

// Free bank offers can produce balances larger than the original HUD box.
// Keep its source position and font face; reduce only overflowing cash labels.
fn fitting_cash_label(
    n: &warzone_app_client::interface::Node,
    font: &Font<'_>,
) -> Option<warzone_app_client::interface::Node> {
    if n.kind != "text"
        || !matches!(
            n.id.as_str(),
            "realMoney" | "hud_real_money_txt" | "real_money_txt"
        )
        || n.text.is_empty()
    {
        return None;
    }
    let width = source_advance(font, &n.text, n.font_size as f32, n.letter_spacing);
    let available = (n.rect[2] - 2.).max(1.);
    if width <= available {
        return None;
    }
    let factor = available / width;
    let mut fitted = n.clone();
    fitted.font_size *= factor;
    fitted.line_height *= factor;
    fitted.letter_spacing *= factor;
    for e in [&mut fitted.shadow, &mut fitted.glow].into_iter().flatten() {
        e.blur_x *= factor;
        e.blur_y *= factor;
        e.offset_x *= factor;
        e.offset_y *= factor;
    }
    Some(fitted)
}

// GraphicsManager0xd3529/0xd5469 maps JSON `glow` to MTLabel's CoreText
// stroke, not a glow filter. StrokeWidth is fixed at10 percent of the EM.
// Shadow opacity is strength if strength<1, otherwise alpha; multiplying them
// made the imported half-opacity HUD shadows fully opaque at strength2.
fn source_shadow_alpha(effect: &warzone_app_client::interface::TextEffect) -> f64 {
    if effect.strength < 1. {
        effect.strength.clamp(0., 1.)
    } else {
        effect.alpha.clamp(0., 1.)
    }
}
fn mask_sample(mask: &[u8], width: usize, height: usize, x: f64, y: f64) -> f64 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let dx = x - ix as f64;
    let dy = y - iy as f64;
    let at = |x: i32, y: i32| {
        if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
            mask[y as usize * width + x as usize] as f64 / 255.
        } else {
            0.
        }
    };
    at(ix, iy) * (1. - dx) * (1. - dy)
        + at(ix + 1, iy) * dx * (1. - dy)
        + at(ix, iy + 1) * (1. - dx) * dy
        + at(ix + 1, iy + 1) * dx * dy
}
fn source_stroke_mask(mask: &[u8], width: usize, height: usize, points: f64) -> Vec<u8> {
    let radius = (points * 0.05).clamp(0., 32.);
    let offsets: Vec<_> = (0..32)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 32.;
            [radius * a.cos(), radius * a.sin()]
        })
        .collect();
    (0..mask.len())
        .map(|k| {
            let x = (k % width) as f64;
            let y = (k / width) as f64;
            let coverage = offsets
                .iter()
                .map(|p| mask_sample(mask, width, height, x + p[0], y + p[1]))
                .fold(mask[k] as f64 / 255., f64::max);
            (coverage * 255.).round().clamp(0., 255.) as u8
        })
        .collect()
}
fn source_shadow_mask(
    mask: &[u8],
    width: usize,
    height: usize,
    effect: &warzone_app_client::interface::TextEffect,
) -> Vec<u8> {
    // CoreGraphics' platform-specific raster kernel is not exposed by the
    // original client. Use a bounded separable Gaussian for the native mask;
    // the recovered blurX, offset and opacity semantics are preserved. The
    // resulting glyph-edge pixels remain a platform parity limitation.
    let blur = effect.blur_x.abs().min(16.);
    let radius = (blur * 2.).ceil() as i32;
    let mut kernel: Vec<f64> = (-radius..=radius)
        .map(|i| {
            if blur > 0. {
                (-0.5 * (i as f64 / (blur * 0.5)).powi(2)).exp()
            } else {
                1.
            }
        })
        .collect();
    let sum = kernel.iter().sum::<f64>();
    for v in &mut kernel {
        *v /= sum;
    }
    let mut horizontal = vec![0u8; mask.len()];
    let mut blurred = vec![0u8; mask.len()];
    for y in 0..height {
        for x in 0..width {
            let sample = kernel
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    mask_sample(
                        mask,
                        width,
                        height,
                        x as f64 + i as f64 - radius as f64,
                        y as f64,
                    ) * w
                })
                .sum::<f64>();
            horizontal[y * width + x] = (sample * 255.).round() as u8;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let sample = kernel
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    mask_sample(
                        &horizontal,
                        width,
                        height,
                        x as f64,
                        y as f64 + i as f64 - radius as f64,
                    ) * w
                })
                .sum::<f64>();
            blurred[y * width + x] = (sample * 255.).round() as u8;
        }
    }
    (0..mask.len())
        .map(|k| {
            (mask_sample(
                &blurred,
                width,
                height,
                (k % width) as f64 - effect.offset_x,
                (k / width) as f64 - effect.offset_y,
            ) * 255.)
                .round() as u8
        })
        .collect()
}

// MTLabel uses CTTypesetterSuggestLineBreak. Preserve explicit hyphens as
// legal opportunities before falling back to a Unicode-scalar split of an
// overlong fragment. Verified against the original RU mercenary card.
use warzone_app_client::interface_text::wrap_source_text;

fn source_clip(
    canvas: &mut Canvas<Window>,
    clip: impl Into<sdl2::render::ClippingRect>,
) -> Result<(), String> {
    let clip = clip.into();
    if canvas.clip_rect() != clip {
        // SDL2's batched OpenGL path on the observed NVIDIA driver loses some
        // glyph draws when successive source controls change clipping. The
        // same commands render correctly with batching0/software. Flush only
        // at source clip changes; world sprite batching remains intact.
        // SAFETY: raw() belongs to this live, exclusively borrowed Canvas.
        if unsafe { sdl2::sys::SDL_RenderFlush(canvas.raw()) } != 0 {
            return Err(sdl2::get_error());
        }
        canvas.set_clip_rect(clip);
    }
    Ok(())
}

pub fn navigation_hit(scene: &Scene, camera: &Camera, x: i32, y: i32) -> Option<crate::ui::Kind> {
    let kinds = [
        crate::ui::Kind::Construction,
        crate::ui::Kind::Shop,
        crate::ui::Kind::Inventory,
        crate::ui::Kind::Quests,
    ];
    warzone_app_client::hud::navigation(scene, camera)
        .iter()
        .zip(kinds)
        .find_map(|(r, k)| {
            let [rx, ry, w, h] = *r;
            (x >= rx && x < rx + w && y >= ry && y < ry + h).then_some(k)
        })
}
pub struct ModalLayout {
    pub panel: Rect,
    pub close: Rect,
    pub previous: Rect,
    pub next: Rect,
    pub claim: Rect,
}
pub fn modal_layout(camera: &Camera) -> ModalLayout {
    let width = camera.width.saturating_sub(32).min(850);
    let height = camera.height.saturating_sub(70).min(520);
    let x = (camera.width - width) as i32 / 2;
    let y = (camera.height - height) as i32 / 2;
    ModalLayout {
        panel: Rect::new(x, y, width, height),
        close: Rect::new(x + width as i32 - 48, y + 8, 36, 30),
        previous: Rect::new(x + 18, y + height as i32 - 44, 90, 32),
        next: Rect::new(x + 122, y + height as i32 - 44, 90, 32),
        claim: Rect::new(x + 18, y + height as i32 - 82, width.saturating_sub(36), 32),
    }
}
pub struct PanelView {
    pub section: String,
    pub primary_label: String,
    pub title: String,
    pub rows: Vec<String>,
    pub awards: String,
    pub index: usize,
    pub total: usize,
    pub ready: bool,
}
fn speedup_group_frames(bg: [f64; 4], left: f64, icon: f64, right: f64) -> [f64; 6] {
    let inset = 7. * bg[3] / 40.;
    let available = (bg[2] - 2. * inset).max(0.);
    // Native raster effects add padding beyond UIKit's textWidth. Fit only the
    // action label when needed; preserve the whole numeric price and icon.
    // PKUtils.centerButtonLabelL measures both complete CTLines. It never
    // shortens the action caption to reserve numeric price space.
    let _ = available;
    let x = bg[0] + (bg[2] - left - icon - right) * 0.5;
    // PKUtils pins the outer labels on overflow; the original intentionally
    // permits icon/label overlap, but never truncates the action string.
    let overflow = left + icon + right > available;
    [
        if overflow { bg[0] + inset } else { x },
        left,
        x + left,
        icon,
        if overflow {
            bg[0] + bg[2] - inset - right
        } else {
            x + left + icon
        },
        right,
    ]
}
fn source_control_path(n: &warzone_app_client::interface::Node) -> Vec<&str> {
    n.parents
        .iter()
        .map(String::as_str)
        .filter(|p| !p.starts_with("source_label_scale_") && !p.starts_with("source_top_margin_"))
        .collect()
}
pub struct Hud<'a> {
    pub original: Option<&'a warzone_app_client::interface::View>,
    pub world_markers: Option<&'a warzone_app_client::interface::View>,
    pub motion: Option<(&'a warzone_app_client::battle_motion::Playback, f64)>,
    pub tactical: bool,
    pub navigation: Option<[String; 4]>,
    pub zones: Vec<[f64; 4]>,
    pub preview: Option<(&'a Entity, bool)>,
    pub editing: bool,
    pub resources: &'a BTreeMap<String, i64>,
    pub energy: String,
    pub values: warzone_app_client::hud::Values,
    pub quests: Option<PanelView>,
    pub detail: &'a str,
    pub action: &'a str,
    pub message: &'a str,
}

const ARIAL: usize = 0;
const ARIAL_BOLD: usize = 1;
const BODY: usize = 2;
const BUTTON: usize = 3;
const TITLE: usize = 4;
const LEVEL: usize = 5;
const PT: usize = 6;
const PT_BOLD: usize = 7;
const BODY_BOLD: usize = 8;
const IMPACT: usize = 9;
const BODY_ITALIC: usize = 10;
const BODY_BOLD_ITALIC: usize = 11;
const CJK_FALLBACK: usize = 12;
const BUTTON_MEDIUM: usize = 13;
const HEAVY: usize = 14;
const HEAVY_ITALIC: usize = 15;
const PT_ITALIC: usize = 16;
const PT_BOLD_ITALIC: usize = 17;
fn source_face(n: &warzone_app_client::interface::Node) -> usize {
    if n.text
        .chars()
        .any(|c| (0x2e80..=0x9fff).contains(&(c as u32)) || (0xf900..=0xfaff).contains(&(c as u32)))
    {
        return CJK_FALLBACK;
    }
    if n.font_family.contains("AGAalen") {
        LEVEL
    } else if n.font_family == "Impact" {
        IMPACT
    } else if n.font_family.contains("PT Sans") {
        if n.italic {
            if n.bold { PT_BOLD_ITALIC } else { PT_ITALIC }
        } else if n.bold {
            PT_BOLD
        } else {
            PT
        }
    } else if n.font_family.contains("a_DomIno") {
        TITLE
    } else if n.font_family == "Franklin Gothic Medium Cond" {
        BUTTON_MEDIUM
    } else if n.font_family == "Franklin Gothic Heavy" {
        // Fonts.getFontInternalName::: keeps Heavy when bold is requested,
        // even when the source text also has its italic flag set.
        if n.italic && !n.bold {
            HEAVY_ITALIC
        } else {
            HEAVY
        }
    } else if n.font_family.contains("Franklin") {
        BUTTON
    } else if n.font_family.contains("Candara") {
        if n.italic {
            return if n.bold {
                BODY_BOLD_ITALIC
            } else {
                BODY_ITALIC
            };
        }
        if n.bold || n.font_family.contains("Bold") {
            BODY_BOLD
        } else {
            BODY
        }
    } else if n.bold {
        ARIAL_BOLD
    } else {
        ARIAL
    }
}
struct SourceViewCache<'a> {
    key: Vec<u8>,
    size: (u32, u32),
    texture: Texture<'a>,
    trace: Vec<serde_json::Value>,
}
struct GroundCache<'a> {
    size: (u32, u32),
    camera: [u64; 3],
    texture: Texture<'a>,
    terrain: Vec<warzone_app_client::TerrainSprite>,
    details: Vec<warzone_app_client::TerrainSprite>,
    detail_layer: Option<usize>,
    draws: usize,
}
struct PausedWorld<'a> {
    size: (u32, u32),
    camera: [u64; 3],
    texture: Texture<'a>,
}
pub struct Art<'a> {
    paused_world: Option<PausedWorld<'a>>,
    paused_hits: u64,
    paused_misses: u64,
    pub cache_ground: bool,
    ground: Option<GroundCache<'a>>,
    ground_hits: u64,
    ground_misses: u64,
    pub cache_source_views: bool,
    source_view: Option<SourceViewCache<'a>>,
    source_view_key: Vec<u8>,
    source_view_hits: u64,
    source_view_misses: u64,
    line_layouts: std::cell::RefCell<HashMap<String, Vec<String>>>,
    premultiplied: HashSet<String>,
    pub trace_source_buttons: bool,
    source_button_trace: Vec<serde_json::Value>,
    working_playback: warzone_app_client::world::WorkingPlayback,
    textures: HashMap<String, Texture<'a>>,
    image_info: HashMap<String, warzone_app_client::ImageInfo>,
    pack: PathBuf,
    texture_used: HashMap<String, u64>,
    warm_ui: std::collections::HashSet<String>,
    texture_bytes: u64,
    texture_hits: u64,
    texture_decodes: u64,
    texture_evictions: u64,
    frame: u64,
    fonts: Vec<Font<'static>>,
    ink: u32,
    styled: bool,
    text: HashMap<String, Texture<'a>>,
    label_used: HashMap<String, u64>,
    label_clock: u64,
    label_bytes: u64,
    label_hits: u64,
    label_rasters: u64,
    label_evictions: u64,
    creator: &'a TextureCreator<WindowContext>,
}
impl<'a> Art<'a> {
    pub fn load(
        creator: &'a TextureCreator<WindowContext>,
        scene: &Scene,
        pack: &Path,
    ) -> Result<Self, String> {
        let bytes = std::fs::read(contained_file(pack, &scene.font)?).map_err(|e| e.to_string())?;
        let font = Font::try_from_vec(bytes).ok_or("Invalid font")?;
        let bold_font = if let Some(hud) = &scene.hud {
            let bytes =
                std::fs::read(contained_file(pack, &hud.bold_font)?).map_err(|e| e.to_string())?;
            Font::try_from_vec(bytes).ok_or("Invalid HUD bold font")?
        } else {
            font.clone()
        };
        let mut fonts = vec![
            font.clone(),
            bold_font,
            font.clone(),
            font.clone(),
            font.clone(),
            font,
        ];
        fonts.resize(18, fonts[BODY].clone());
        if let Some(p) = &scene.presentation {
            for (index, role) in [
                (CJK_FALLBACK, "cjk_fallback"),
                (BODY, "body"),
                (BUTTON, "button"),
                (TITLE, "title"),
                (LEVEL, "level"),
                (PT, "pt"),
                (PT_BOLD, "pt_bold"),
                (BODY_BOLD, "body_bold"),
                (IMPACT, "impact"),
                (BODY_ITALIC, "body_italic"),
                (BODY_BOLD_ITALIC, "body_bold_italic"),
                (BUTTON_MEDIUM, "button_medium"),
                (HEAVY, "heavy"),
                (HEAVY_ITALIC, "heavy_italic"),
                (PT_ITALIC, "pt_italic"),
                (PT_BOLD_ITALIC, "pt_bold_italic"),
            ] {
                let Some(face) = p.fonts.get(role) else {
                    // Older published packs remain usable with their previous
                    // fallback. Current packs carry the exact original faces.
                    let fallback = match index {
                        BUTTON_MEDIUM | HEAVY | HEAVY_ITALIC => BUTTON,
                        PT_ITALIC => PT,
                        PT_BOLD_ITALIC => PT_BOLD,
                        _ => index,
                    };
                    fonts[index] = fonts[fallback].clone();
                    continue;
                };
                let bytes =
                    std::fs::read(contained_file(pack, &face.path)?).map_err(|e| e.to_string())?;
                fonts[index] =
                    Font::try_from_vec(bytes).ok_or("Invalid original interface font")?;
            }
        }
        Ok(Self {
            paused_world: None,
            paused_hits: 0,
            paused_misses: 0,
            cache_ground: true,
            ground: None,
            ground_hits: 0,
            ground_misses: 0,
            cache_source_views: true,
            source_view: None,
            source_view_key: Vec::new(),
            source_view_hits: 0,
            source_view_misses: 0,
            line_layouts: std::cell::RefCell::new(HashMap::new()),
            premultiplied: HashSet::new(),
            trace_source_buttons: false,
            source_button_trace: Vec::new(),
            working_playback: Default::default(),
            textures: HashMap::new(),
            image_info: scene.textures.clone(),
            pack: pack.to_owned(),
            texture_used: HashMap::new(),
            warm_ui: Default::default(),
            texture_bytes: 0,
            texture_hits: 0,
            texture_decodes: 0,
            texture_evictions: 0,
            frame: 0,
            fonts,
            ink: 0xf5f0d5,
            styled: scene.presentation.is_some(),
            text: HashMap::new(),
            label_used: HashMap::new(),
            label_clock: 0,
            label_bytes: 0,
            label_hits: 0,
            label_rasters: 0,
            label_evictions: 0,
            creator,
        })
    }
    // Keep the lazy loader, with bounded warm residency across window changes.
    // Two-frame eviction forced every returning dialog to decode/upload again.
    fn evict_texture(&mut self, key: &str) {
        if let Some(texture) = self.textures.remove(key) {
            let q = texture.query();
            self.texture_bytes -= q.width as u64 * q.height as u64 * 4;
            self.texture_evictions += 1;
        }
        self.texture_used.remove(key);
        self.premultiplied.remove(key);
    }
    fn reserve_texture(&mut self, bytes: u64) -> Result<(), String> {
        const BUDGET: u64 = 96 * 1024 * 1024;
        if bytes > BUDGET {
            return Err("Source atlas exceeds bounded96MiB texture budget".into());
        }
        while self.texture_bytes + bytes > BUDGET || self.textures.len() >= 192 {
            // Never destroy an atlas already queued for this frame's draw.
            let victim = self
                .textures
                .keys()
                .filter(|k| self.texture_used.get(*k).copied().unwrap_or(0) < self.frame)
                .min_by_key(|k| self.texture_used.get(*k).copied().unwrap_or(0))
                .cloned();
            let Some(key) = victim else {
                return Err("Visible atlases exceed bounded96MiB texture budget".into());
            };
            self.evict_texture(&key);
        }
        Ok(())
    }
    fn ensure_texture(&mut self, key: &str) -> Result<(), String> {
        self.ensure_texture_pixels(key, key, false)
    }
    fn ensure_texture_pixels(
        &mut self,
        key: &str,
        source: &str,
        grayscale: bool,
    ) -> Result<(), String> {
        if let Some(used) = self.texture_used.get_mut(key) {
            *used = self.frame;
        } else {
            self.texture_used.insert(key.to_owned(), self.frame);
        }
        if self.textures.contains_key(key) {
            self.texture_hits += 1;
            return Ok(());
        }
        let expected = self
            .image_info
            .get(source)
            .ok_or("Unknown texture")?
            .clone();
        let atlas_bytes = expected.width as u64 * expected.height as u64 * 4;
        self.reserve_texture(atlas_bytes)?;
        let path = contained_file(&self.pack, source)?;
        let mut decoder =
            png::Decoder::new(BufReader::new(File::open(path).map_err(|e| e.to_string())?));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("PNG is too large")?];
        let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
        if (info.width, info.height) != (expected.width, expected.height) {
            return Err("PNG metadata mismatch".into());
        }
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        let bytes = &pixels[..info.buffer_size()];
        match info.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(bytes),
            png::ColorType::Rgb => {
                for p in bytes.chunks_exact(3) {
                    rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
            png::ColorType::Grayscale => {
                for &v in bytes {
                    rgba.extend_from_slice(&[v, v, v, 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for p in bytes.chunks_exact(2) {
                    rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
                }
            }
            _ => return Err("PNG palette was not expanded".into()),
        }
        let mut texture = self
            .creator
            .create_texture_static(PixelFormatEnum::RGBA32, info.width, info.height)
            .map_err(|e| e.to_string())?;
        let premultiplied = source_premultiplied_blend(&mut texture);
        if premultiplied {
            self.premultiplied.insert(key.to_owned());
        } else {
            texture.set_blend_mode(sdl2::render::BlendMode::Blend);
        }
        if key.starts_with("#source_highlight:") {
            source_highlight_pixels(&mut rgba);
        }
        if expected.sparrow_rgba4444 {
            source_sparrow_pixels(&mut rgba, grayscale, premultiplied);
        } else {
            source_image_pixels(&mut rgba, grayscale, premultiplied);
        }
        texture
            .update(None, &rgba, info.width as usize * 4)
            .map_err(|e| e.to_string())?;
        // SPGLTexture.init explicitly calls setFilter:1. That chooses GL_LINEAR
        // for magnification and linear sampling for minification (0x2166b5),
        // whereas SDL's default nearest sampling pixelated the recovered art.
        texture.set_scale_mode(if premultiplied {
            sdl2::render::ScaleMode::Linear
        } else {
            sdl2::render::ScaleMode::Nearest
        });
        self.textures.insert(key.to_owned(), texture);
        self.texture_bytes += atlas_bytes;
        self.texture_decodes += 1;
        Ok(())
    }
    pub fn texture_report(&self) -> serde_json::Value {
        let bytes: u64 = self
            .textures
            .keys()
            .map(|k| {
                let i = &self.image_info[k.strip_prefix("#gray:").unwrap_or(k)];
                i.width as u64 * i.height as u64 * 4
            })
            .sum();
        serde_json::json!({"paused_world_cache_hits":self.paused_hits,"paused_world_cache_misses":self.paused_misses,"paused_world_rgba_bytes":self.paused_world.as_ref().map_or(0,|p|p.size.0 as u64*p.size.1 as u64*4),"ground_cache_hits":self.ground_hits,"ground_cache_misses":self.ground_misses,"ground_cache_rgba_bytes":self.ground.as_ref().map_or(0,|c|c.size.0 as u64*c.size.1 as u64*4),"ground_cache_budget_bytes":32*1024*1024,"loaded":self.textures.len(),"catalog":self.image_info.len(),"rgba_bytes":bytes,"texture_budget_bytes":96*1024*1024,"texture_limit":192,"texture_hits":self.texture_hits,"texture_decodes":self.texture_decodes,"texture_evictions":self.texture_evictions,"premultiplied_alpha_textures":self.textures.keys().filter(|k|self.premultiplied.contains(*k)).count(),"lazy":true,"stale_frame_eviction":true,"stale_frame_ttl":1800,"label_cache_entries":self.text.len(),"label_entries":self.label_used.len(),"label_rgba_bytes":self.label_bytes,"label_budget_bytes":24*1024*1024,"label_limit":512,"label_hits":self.label_hits,"label_rasters":self.label_rasters,"label_evictions":self.label_evictions,"tutorial_mask_entries":self.text.keys().filter(|k|k.starts_with("source_tutorial_mask:")).count(),"source_view_cache_hits":self.source_view_hits,"source_view_cache_misses":self.source_view_misses,"source_view_cache_rgba_bytes":self.source_view.as_ref().map_or(0,|c|c.size.0 as u64*c.size.1 as u64*4)})
    }
    /// Evict individual cold labels. Clearing the whole map at256 entries
    /// repeatedly rasterized still-visible HUD and dialog text during browsing.
    /// Source masks remain in their separately bounded one-current-mask slot.
    fn insert_label(&mut self, key: String, texture: Texture<'a>) -> Result<(), String> {
        const BUDGET: u64 = 24 * 1024 * 1024;
        let q = texture.query();
        let bytes = q.width as u64 * q.height as u64 * 4;
        if bytes > BUDGET {
            return Err("Label raster exceeds bounded24MiB cache budget".into());
        }
        while self.label_used.len() >= 512 || self.label_bytes + bytes > BUDGET {
            let Some(old) = self
                .label_used
                .iter()
                .min_by_key(|(_, used)| **used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(texture) = self.text.remove(&old) {
                let q = texture.query();
                self.label_bytes -= q.width as u64 * q.height as u64 * 4;
            }
            self.label_used.remove(&old);
            self.label_evictions += 1;
        }
        self.label_bytes += bytes;
        self.text.insert(key.clone(), texture);
        self.label_used.insert(key, self.label_clock);
        self.label_rasters += 1;
        Ok(())
    }
    fn cache_label(
        &mut self,
        text: &str,
        size: f32,
        face: usize,
        color: u32,
    ) -> Result<String, String> {
        self.cache_styled_label(text, size, face, color, None)
    }
    fn cache_styled_label(
        &mut self,
        text: &str,
        size: f32,
        face: usize,
        color: u32,
        style: Option<&warzone_app_client::interface::Node>,
    ) -> Result<String, String> {
        let key = format!(
            "{size:.1}:{face}:{color}:{text}:{:?}",
            style.map(|n| (n.letter_spacing, &n.shadow, &n.glow))
        );
        self.label_clock = self.label_clock.saturating_add(1);
        if self.text.contains_key(&key) {
            self.label_used.insert(key.clone(), self.label_clock);
            self.label_hits += 1;
            return Ok(key);
        }
        if !self.text.contains_key(&key) {
            let font = &self.fonts[face];
            let scale = if style.is_some() {
                source_text_scale(font, size)
            } else {
                Scale::uniform(size)
            };
            let vm = font.v_metrics(scale);
            let pad = source_label_pad(size, face, style);
            let mut glyphs: Vec<_> = font
                .layout(text, scale, point(pad, vm.ascent + pad))
                .collect();
            if let Some(n) = style {
                for (i, g) in glyphs.iter_mut().enumerate() {
                    let p = g.position();
                    g.set_position(point(p.x + i as f32 * n.letter_spacing as f32, p.y));
                }
            }
            let width = (glyphs
                .iter()
                .filter_map(|g| g.pixel_bounding_box())
                .map(|b| b.max.x)
                .max()
                .unwrap_or(1)
                + pad as i32
                + 1)
            .max(1) as u32;
            let height = (vm.ascent - vm.descent + pad * 2. + 2.).ceil() as u32;
            let pixels = (width as u64)
                .checked_mul(height as u64)
                .filter(|n| *n <= 24 * 1024 * 1024 / 4)
                .ok_or("Label raster exceeds bounded24MiB cache budget")?;
            let mut mask = vec![0u8; pixels as usize];
            for g in glyphs {
                if let Some(bb) = g.pixel_bounding_box() {
                    g.draw(|x, y, v| {
                        let xx = x as i32 + bb.min.x;
                        let yy = y as i32 + bb.min.y;
                        if xx >= 0 && yy >= 0 && (xx as u32) < width && (yy as u32) < height {
                            let k = yy as usize * width as usize + xx as usize;
                            mask[k] = mask[k].max((v * 255.) as u8);
                        }
                    });
                }
            }
            let stroke = style
                .filter(|n| n.glow.is_some())
                .map(|_| source_stroke_mask(&mask, width as usize, height as usize, size as f64));
            let shadow = style.and_then(|n| n.shadow.as_ref()).map(|e| {
                source_shadow_mask(
                    stroke.as_deref().unwrap_or(&mask),
                    width as usize,
                    height as usize,
                    e,
                )
            });
            let fill_shadow = style
                .filter(|n| n.glow.is_some())
                .and_then(|n| n.shadow.as_ref())
                .map(|e| source_shadow_mask(&mask, width as usize, height as usize, e));
            let mut rgba = vec![0u8; mask.len() * 4];
            for y in 0..height as i32 {
                for x in 0..width as i32 {
                    let k = (y as u32 * width + x as u32) as usize;
                    let mut alpha = mask[k];
                    let mut rgb = color;
                    if face == TITLE && style.is_none_or(|n| n.shadow.is_none() && n.glow.is_none())
                    {
                        let mut edge = 0;
                        for dy in -2..=2 {
                            for dx in -2..=2 {
                                let (px, py) = (x + dx, y + dy);
                                if px >= 0 && py >= 0 && px < width as i32 && py < height as i32 {
                                    edge = edge.max(mask[(py as u32 * width + px as u32) as usize]);
                                }
                            }
                        }
                        if alpha == 0 {
                            alpha = edge;
                            rgb = 0x271f15;
                        }
                    }
                    let mut out = [0f64; 4];
                    let mut over = |rgb: u32, a: f64| {
                        let remain = 1. - a;
                        for (i, shift) in [16, 8, 0].into_iter().enumerate() {
                            out[i] = ((rgb >> shift) & 255) as f64 * a + out[i] * remain;
                        }
                        out[3] = a + out[3] * remain;
                    };
                    if let Some(n) = style {
                        if let (Some(e), Some(mask)) = (&n.shadow, &shadow) {
                            over(e.color, mask[k] as f64 / 255. * source_shadow_alpha(e));
                        }
                        if let (Some(e), Some(mask)) = (&n.glow, &stroke) {
                            over(e.color, mask[k] as f64 / 255.);
                            // MTLabel draws the stroked+filled line, then the
                            // ordinary filled line, both with CGContext shadow.
                            over(rgb, alpha as f64 / 255.);
                            if let (Some(e), Some(mask)) = (&n.shadow, &fill_shadow) {
                                over(e.color, mask[k] as f64 / 255. * source_shadow_alpha(e));
                            }
                        }
                    }
                    over(rgb, alpha as f64 / 255.);
                    let a = out[3];
                    rgba[k * 4..k * 4 + 4].copy_from_slice(&[
                        if a > 0. { (out[0] / a) as u8 } else { 0 },
                        if a > 0. { (out[1] / a) as u8 } else { 0 },
                        if a > 0. { (out[2] / a) as u8 } else { 0 },
                        (a * 255.) as u8,
                    ]);
                }
            }
            let mut texture = self
                .creator
                .create_texture_static(PixelFormatEnum::RGBA32, width, height)
                .map_err(|e| e.to_string())?;
            texture
                .update(None, &rgba, width as usize * 4)
                .map_err(|e| e.to_string())?;
            texture.set_blend_mode(sdl2::render::BlendMode::Blend);
            self.insert_label(key.clone(), texture)?;
        }
        Ok(key)
    }
    fn label(
        &mut self,
        canvas: &mut Canvas<Window>,
        text: &str,
        x: i32,
        y: i32,
    ) -> Result<(), String> {
        let key = self.cache_label(text, 20., BODY, self.ink)?;
        let t = &self.text[&key];
        let q = t.query();
        canvas.copy(t, None, Some(Rect::new(x, y, q.width, q.height)))
    }
    fn fit_label(
        &mut self,
        canvas: &mut Canvas<Window>,
        text: &str,
        area: Rect,
    ) -> Result<(), String> {
        self.fit_text(canvas, text, area, 20., BODY, self.ink)
    }
    fn fit_text(
        &mut self,
        canvas: &mut Canvas<Window>,
        text: &str,
        area: Rect,
        size: f32,
        face: usize,
        color: u32,
    ) -> Result<(), String> {
        let key = self.cache_label(text, size, face, color)?;
        let t = &self.text[&key];
        let q = t.query();
        let fit = (area.width() as f64 / q.width as f64)
            .min(area.height() as f64 / q.height as f64)
            .min(1.0);
        canvas.copy(
            t,
            None,
            Some(Rect::new(
                area.x(),
                area.y(),
                (q.width as f64 * fit).max(1.) as u32,
                (q.height as f64 * fit).max(1.) as u32,
            )),
        )
    }
    fn skin(
        &mut self,
        canvas: &mut Canvas<Window>,
        key: &str,
        area: Rect,
        border: u32,
    ) -> Result<(), String> {
        self.ensure_texture(key)?;
        let t = &self.textures[key];
        let q = t.query();
        let b = border
            .min(q.width / 2)
            .min(q.height / 2)
            .min(area.width() / 2)
            .min(area.height() / 2);
        let xs = [0, b, q.width - b, q.width];
        let ys = [0, b, q.height - b, q.height];
        let dx = [
            area.x(),
            area.x() + b as i32,
            area.right() - b as i32,
            area.right(),
        ];
        let dy = [
            area.y(),
            area.y() + b as i32,
            area.bottom() - b as i32,
            area.bottom(),
        ];
        for y in 0..3 {
            for x in 0..3 {
                let (w, h) = (dx[x + 1] - dx[x], dy[y + 1] - dy[y]);
                if w > 0 && h > 0 {
                    canvas.copy(
                        t,
                        Some(Rect::new(
                            xs[x] as i32,
                            ys[y] as i32,
                            xs[x + 1] - xs[x],
                            ys[y + 1] - ys[y],
                        )),
                        Some(Rect::new(dx[x], dy[y], w as u32, h as u32)),
                    )?;
                }
            }
        }
        Ok(())
    }
    fn button(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        area: Rect,
        text: &str,
        ready: bool,
    ) -> Result<(), String> {
        if let Some(p) = &scene.presentation {
            self.skin(canvas, &p.button, area, 8)?;
            if !ready {
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 150));
                canvas.fill_rect(area)?;
            }
        } else {
            canvas.set_draw_color(if ready {
                Color::RGB(79, 110, 30)
            } else {
                Color::RGB(55, 61, 46)
            });
            canvas.fill_rect(area)?;
        }
        self.fit_text(
            canvas,
            text,
            Rect::new(
                area.x() + 8,
                area.y() + 3,
                area.width().saturating_sub(16),
                area.height().saturating_sub(6),
            ),
            20.,
            BUTTON,
            if self.styled { 0xffffff } else { self.ink },
        )
    }
    fn original_hud(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        camera: &Camera,
        values: &warzone_app_client::hud::Values,
    ) -> Result<(), String> {
        let Some(hud) = &scene.hud else { return Ok(()) };
        let s = warzone_app_client::hud::scale(camera);
        let old_clip = canvas.clip_rect();
        canvas.set_clip_rect(Rect::new(
            0,
            0,
            camera.width,
            warzone_app_client::hud::height(scene, camera) as u32,
        ));
        for n in &hud.nodes {
            let [x, y, w, h] = n.rect;
            if n.kind == "image" {
                self.ensure_texture(&n.texture)?;
                let t = &self.textures[&n.texture];
                let q = t.query();
                let r = warzone_app_client::hud::image_rect(n, camera);
                let p = if n.binding.is_empty() {
                    1.0
                } else {
                    values.progress.get(&n.binding).copied().unwrap_or(0.0)
                };
                if p > 0.0 {
                    canvas.copy_f(
                        t,
                        Some(Rect::new(
                            0,
                            0,
                            ((q.width as f64 * p).round() as u32).max(1),
                            q.height,
                        )),
                        Some(FRect::new(
                            r[0] as f32,
                            r[1] as f32,
                            (r[2] * p) as f32,
                            r[3] as f32,
                        )),
                    )?;
                }
            } else if let Some(text) = values.texts.get(&n.binding).filter(|t| !t.is_empty()) {
                // MTLabel consumes an EM point size. The legacy HUD path used
                // rusttype's ascent/descent size and shrank it again to the box.
                let mut node = warzone_app_client::interface::Node {
                    source: None,
                    id: n.binding.clone(),
                    group: String::new(),
                    kind: "text".into(),
                    rect: [x * s, y * s, w * s, h * s],
                    rotation: 0.,
                    texture: String::new(),
                    text: text.clone(),
                    font_family: n.font_family.clone(),
                    font_size: n.font_size * s,
                    bold: n.bold,
                    italic: false,
                    flip_x: false,
                    align: n.align.clone(),
                    line_height: n.font_size * 1.2 * s,
                    multiline: false,
                    color: n.color,
                    parents: vec![format!("source_label_scale_{s}")],
                    letter_spacing: 0.,
                    shadow: n.shadow.clone(),
                    glow: n.glow.clone(),
                };
                for e in [&mut node.shadow, &mut node.glow].into_iter().flatten() {
                    e.blur_x *= s;
                    e.blur_y *= s;
                    e.offset_x *= s;
                    e.offset_y *= s;
                }
                let mut labels = warzone_app_client::interface::View::default();
                labels.draws.push(warzone_app_client::interface::Draw {
                    node,
                    clip: None,
                    progress: 1.,
                    opacity: 255,
                    grayscale: false,
                });
                self.source_interface(canvas, &labels)?;
            }
        }
        canvas.set_clip_rect(old_clip);
        Ok(())
    }
    fn sprite(
        &mut self,
        canvas: &mut Canvas<Window>,
        camera: &Camera,
        texture: &str,
        src: [u32; 4],
        r: [f64; 4],
        origin: [f64; 2],
    ) -> Result<bool, String> {
        let p = camera.screen([r[0] + origin[0], r[1] + origin[1]]);
        let w = r[2] * camera.zoom;
        let h = r[3] * camera.zoom;
        if p[0] + w < 0.
            || p[1] + h < 0.
            || p[0] >= camera.width as f64
            || p[1] >= camera.height as f64
        {
            return Ok(false);
        }
        self.ensure_texture(texture)?;
        canvas.copy_f(
            &self.textures[texture],
            Some(Rect::new(src[0] as i32, src[1] as i32, src[2], src[3])),
            Some(FRect::new(p[0] as f32, p[1] as f32, w as f32, h as f32)),
        )?;
        Ok(true)
    }
    fn sale_sprite(
        &mut self,
        canvas: &mut Canvas<Window>,
        camera: &Camera,
        texture: &str,
        src: [u32; 4],
        rect: [f64; 4],
        origin: [f64; 2],
        red: bool,
    ) -> Result<bool, String> {
        if !red {
            return self.sprite(canvas, camera, texture, src, rect, origin);
        }
        self.ensure_texture(texture)?;
        self.textures
            .get_mut(texture)
            .unwrap()
            .set_color_mod(255, 0, 0);
        let result = self.sprite(canvas, camera, texture, src, rect, origin);
        // Atlases are shared by other entities. Restore even if SDL rejects a copy.
        self.textures
            .get_mut(texture)
            .unwrap()
            .set_color_mod(255, 255, 255);
        result
    }
    fn direct_ground(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        camera: &Camera,
    ) -> Result<usize, String> {
        // SDL window-resize notification and drawable resize can arrive in
        // adjacent frames. Ground always occupies the whole current target.
        canvas.set_viewport(None);
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        let mut draws = 0;
        for i in 0..=scene.terrain.len() {
            if scene.ground_detail_layer == Some(i) {
                for t in &scene.ground_details {
                    draws +=
                        self.sprite(canvas, camera, &t.texture, t.src, t.rect, [0., 0.])? as usize;
                }
            }
            if let Some(t) = scene.terrain.get(i) {
                draws += self.sprite(canvas, camera, &t.texture, t.src, t.rect, [0., 0.])? as usize;
            }
        }
        Ok(draws)
    }
    fn cached_ground(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        camera: &Camera,
    ) -> Result<usize, String> {
        const BUDGET: u64 = 32 * 1024 * 1024;
        canvas.set_viewport(None);
        let size = canvas.output_size()?;
        // Software's window and RGBA target use different intermediate blend
        // rounding. Keep exact direct draws on unverified renderer backends.
        if canvas.info().name != "opengl" {
            self.cache_ground = false;
        }
        if !self.cache_ground || size.0 as u64 * size.1 as u64 * 4 > BUDGET {
            self.ground = None;
            return self.direct_ground(canvas, scene, camera);
        }
        let pose = [
            camera.x.to_bits(),
            camera.y.to_bits(),
            camera.zoom.to_bits(),
        ];
        let mut cached = self.ground.take().filter(|old| old.size == size);
        let same_layers = cached.as_ref().is_some_and(|old| {
            old.terrain == scene.terrain
                && old.details == scene.ground_details
                && old.detail_layer == scene.ground_detail_layer
        });
        if same_layers && cached.as_ref().is_some_and(|old| old.camera == pose) {
            let old = cached.as_ref().unwrap();
            canvas.copy(&old.texture, None, None)?;
            let draws = old.draws;
            self.ground_hits += 1;
            self.ground = cached;
            return Ok(draws);
        }
        if cached.is_none() {
            let Ok(mut texture) =
                self.creator
                    .create_texture_target(PixelFormatEnum::RGBA32, size.0, size.1)
            else {
                return self.direct_ground(canvas, scene, camera);
            };
            // Ground includes the opaque background, so a 1:1 replacement copy
            // introduces neither another alpha blend nor a resampling pass.
            texture.set_blend_mode(sdl2::render::BlendMode::None);
            texture.set_scale_mode(sdl2::render::ScaleMode::Nearest);
            cached = Some(GroundCache {
                size,
                camera: pose,
                texture,
                terrain: Vec::new(),
                details: Vec::new(),
                detail_layer: None,
                draws: 0,
            });
        }
        let mut cached = cached.unwrap();
        let mut rendered = Ok(0);
        let clip = canvas.clip_rect();
        source_clip(canvas, None)?;
        canvas
            .with_texture_canvas(&mut cached.texture, |target| {
                rendered = self.direct_ground(target, scene, camera);
            })
            .map_err(|e| e.to_string())?;
        source_clip(canvas, clip)?;
        let draws = rendered?;
        if !same_layers {
            cached.terrain.clone_from(&scene.terrain);
            cached.details.clone_from(&scene.ground_details);
            cached.detail_layer = scene.ground_detail_layer;
        }
        cached.camera = pose;
        cached.draws = draws;
        self.ground_misses += 1;
        canvas.copy(&cached.texture, None, None)?;
        self.ground = Some(cached);
        Ok(draws)
    }
    pub fn draw(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        camera: &Camera,
        seconds: f64,
        selected: Option<usize>,
        hud_state: Hud<'_>,
    ) -> Result<usize, String> {
        self.draw_with_assets(canvas, scene, scene, camera, seconds, selected, hud_state)
    }
    pub fn draw_with_assets(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        assets: &Scene,
        camera: &Camera,
        seconds: f64,
        selected: Option<usize>,
        hud_state: Hud<'_>,
    ) -> Result<usize, String> {
        self.frame = self.frame.saturating_add(1);
        let frame = self.frame;
        let stale: Vec<_> = self
            .textures
            .keys()
            .filter(|key| {
                !self.warm_ui.contains(key.as_str())
                    && self
                        .texture_used
                        .get(*key)
                        .is_none_or(|used| used.saturating_add(1800) < frame)
            })
            .cloned()
            .collect();
        for key in stale {
            self.evict_texture(&key);
        }

        // SceneMain.onShopDisplay_/onProductionDisplay_ stop SPView while
        // their native UIView is open. Keep one bounded readback of the world,
        // including transparent dialog corners; don't redraw hundreds of units
        // underneath every pixel of a horizontal scroll. Logic still advances.
        let background_capture = warzone_app_client::browser::background_capture();
        let pause = !background_capture
            && !hud_state.tactical
            && hud_state.original.is_some_and(|v| {
                v.draws
                    .iter()
                    .any(|d| d.node.parents.iter().any(|p| p == "source_pause_world"))
            });
        let size = canvas.output_size()?;
        let pose = [
            camera.x.to_bits(),
            camera.y.to_bits(),
            camera.zoom.to_bits(),
        ];
        if !pause
            || self
                .paused_world
                .as_ref()
                .is_some_and(|p| p.size != size || p.camera != pose)
        {
            self.paused_world = None;
        }
        if pause {
            if let Some(p) = &self.paused_world {
                canvas.set_viewport(None);
                canvas.set_clip_rect(None);
                canvas.copy(&p.texture, None, None)?;
                self.paused_hits += 1;
                self.cached_source_interface(canvas, hud_state.original.unwrap())?;
                return Ok(1);
            }
        }
        let mut draws = self.cached_ground(canvas, scene, camera)?;
        // SceneBattle leaves destroyed Building artwork in its ground layer.
        // Living actors and the movement overlay must both cover those ruins.
        let ground_ruin = |e: &warzone_app_client::Entity| {
            hud_state.tactical
                && e.kind == "building"
                && e.archived["state"] == 0
                && hud_state
                    .motion
                    .and_then(|(p, t)| p.sample(&e.uid, t))
                    .is_none_or(|p| p.finished_death)
        };
        for e in scene.entities.iter().filter(|e| ground_ruin(e)) {
            let p = scene.projection.project(e.x, e.y);
            for track in &e.tracks {
                self.ensure_texture(&track.texture)?;
                for part in track.frame(seconds) {
                    draws += self.sprite(canvas, camera, &track.texture, part.src, part.rect, p)?
                        as usize;
                }
            }
        }
        canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
        canvas.set_draw_color(Color::RGBA(80, 200, 255, 190));
        for [x, y, w, h] in &hud_state.zones {
            // Battle's world-only Scene intentionally drops UI metadata; the
            // retained HUD comes from the full asset Scene through Hud.original.
            if hud_state.tactical && hud_state.original.is_some() {
                // SceneBattle.createSoldiersListInMission (0x48271) places
                // one SPQuadEx at Draw.coord2Screen2(x,y); its local corners
                // use coord2Screen(0,0),(w,0),(w,h),(0,h). Source color00cc00
                // and alpha.5 are explicit at0x48796. No cell wire grid.
                let points = [[*x, *y], [x + w, *y], [x + w, y + h], [*x, y + h]];
                let vertices = points.map(|p| {
                    let p = camera.screen(scene.projection.project(p[0], p[1]));
                    Vertex {
                        position: FPoint::new(p[0] as f32, p[1] as f32),
                        color: Color::RGBA(0, 204, 0, 128),
                        tex_coord: FPoint::new(0., 0.),
                    }
                });
                canvas.render_geometry(&vertices, None, &[0, 1, 2, 0, 2, 3])?;
                continue;
            }
            for dx in 0..*w as i32 {
                for dy in 0..*h as i32 {
                    let x = x + dx as f64;
                    let y = y + dy as f64;
                    let points = [
                        [x - 0.5, y - 0.5],
                        [x + 0.5, y - 0.5],
                        [x + 0.5, y + 0.5],
                        [x - 0.5, y + 0.5],
                        [x - 0.5, y - 0.5],
                    ]
                    .map(|p| {
                        let s = camera.screen(scene.projection.project(p[0], p[1]));
                        Point::new(s[0].round() as i32, s[1].round() as i32)
                    });
                    canvas.draw_lines(points.as_slice())?;
                }
            }
        }
        if let Some(v) = hud_state.world_markers {
            for edge in &v.world_boundary {
                source_segment(
                    canvas,
                    edge[0],
                    edge[1],
                    3. * camera.zoom,
                    Color::RGBA(32, 144, 16, 128),
                )?;
            }
        }
        if let Some(v) = hud_state.original.filter(|v| v.retained_battle_selection) {
            self.source_interface_layer(canvas, v, true)?;
            if let Some((quad, valid)) = v.deployment_preview {
                source_quad(
                    canvas,
                    quad,
                    if valid {
                        Color::RGBA(0, 255, 0, 178)
                    } else {
                        Color::RGBA(255, 0, 0, 178)
                    },
                )?;
            }
            for pair in v.battle_route.windows(2) {
                source_segment(
                    canvas,
                    pair[0],
                    pair[1],
                    6. * camera.zoom,
                    Color::RGBA(255, 255, 255, 128),
                )?;
            }
            if let Some(p) = v.battle_route.last() {
                let mut vertices = vec![Vertex {
                    position: FPoint::new(p[0] as f32, p[1] as f32),
                    color: Color::WHITE,
                    tex_coord: FPoint::new(0., 0.),
                }];
                for i in 0..=24 {
                    let a = i as f64 * std::f64::consts::TAU / 24.;
                    vertices.push(Vertex {
                        position: FPoint::new(
                            (p[0] + 16. * camera.zoom * a.cos()) as f32,
                            (p[1] + 8. * camera.zoom * a.sin()) as f32,
                        ),
                        color: Color::WHITE,
                        tex_coord: FPoint::new(0., 0.),
                    });
                }
                let indices: Vec<_> = (1..=24).flat_map(|i| [0, i, i + 1]).collect();
                canvas.render_geometry(&vertices, None, &indices)?;
            }
            let dash = 6. * (1f64 + (32f64 / 62.).powi(2)).sqrt() * camera.zoom;
            for pair in v.battle_attack_area.windows(2) {
                let dx = pair[1][0] - pair[0][0];
                let dy = pair[1][1] - pair[0][1];
                let length = dx.hypot(dy);
                let mut n = 0.;
                while n < length {
                    let end = (n + dash).min(length);
                    source_segment(
                        canvas,
                        [pair[0][0] + dx * n / length, pair[0][1] + dy * n / length],
                        [
                            pair[0][0] + dx * end / length,
                            pair[0][1] + dy * end / length,
                        ],
                        3. * camera.zoom,
                        Color::RGB(255, 0, 0),
                    )?;
                    n += 2. * dash;
                }
            }
        }
        if let Some(v) = hud_state.world_markers {
            if let Some(mut points) = v.base_selection {
                // SceneMain.showSelectionBackground: linear width+20 tween,
                // .3s reverse loop; SPQuadEx expands all four vertices.
                let phase = seconds.rem_euclid(0.6) / 0.3;
                let phase = if phase <= 1. { phase } else { 2. - phase };
                let left = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                let right = points
                    .iter()
                    .map(|p| p[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                let center = (left + right) / 2.;
                let ratio = 1. + 20. * camera.zoom * phase / (right - left).max(1.);
                let cy = points.iter().map(|p| p[1]).sum::<f64>() / 4.;
                // SPQuadEx.setWidth also adjusts its top/bottom vertices.
                for p in &mut points {
                    p[0] = center + (p[0] - center) * ratio;
                    p[1] = cy + (p[1] - cy) * ratio;
                }
                source_quad(canvas, points, Color::RGBA(255, 255, 255, 128))?;
            }
            source_attack_area(canvas, &v.battle_attack_area, camera.zoom)?;
        }
        let ghosts = hud_state
            .motion
            .map_or(&[][..], |(p, _)| p.ghosts.as_slice());
        let mut entities: Vec<_> = scene
            .entities
            .iter()
            .chain(ghosts)
            .map(|e| (e, hud_state.motion.and_then(|(p, t)| p.sample(&e.uid, t))))
            .filter(|(e, p)| {
                if !hud_state.tactical && !scene.entity_in_visible_world(e) {
                    return false;
                }
                !p.as_ref()
                    .is_some_and(|p| p.finished_death && e.kind == "unit")
            })
            .collect();
        entities.sort_by(|(a, pa), (b, pb)| {
            if !hud_state.tactical {
                return warzone_app_client::world::depth_compare(a, b, &scene.projection);
            }
            let (ax, ay) = pa.as_ref().map_or((a.x, a.y), |p| (p.x, p.y));
            let (bx, by) = pb.as_ref().map_or((b.x, b.y), |p| (p.x, p.y));
            let ka = warzone_app_client::world::depth_key_at(a, &scene.projection, ax, ay);
            let kb = warzone_app_client::world::depth_key_at(b, &scene.projection, bx, by);
            ka[0]
                .total_cmp(&kb[0])
                .then_with(|| ka[1].total_cmp(&kb[1]))
                .then_with(|| ka[2].total_cmp(&kb[2]))
        });
        for (e, pose) in &entities {
            if ground_ruin(e) {
                continue;
            }
            if hud_state.preview.is_some_and(|(p, _)| p.uid == e.uid) {
                continue;
            }
            let (x, y) = pose.as_ref().map_or((e.x, e.y), |p| (p.x, p.y));
            let p = scene.projection.project(x, y);
            let tracks = if let (Some(c), Some(p)) = (&assets.combat, pose) {
                warzone_app_client::battle_motion::Playback::tracks(c, p, e)
            } else {
                &e.tracks
            };
            let clock = if pose.as_ref().is_none_or(|p| p.pose == "idle") {
                warzone_app_client::animation::idle_time(&e.uid, seconds)
            } else {
                pose.as_ref().unwrap().time
            };
            let sale_red = !hud_state.tactical
                && hud_state
                    .original
                    .as_ref()
                    .and_then(|v| v.sale_tint_uid.as_deref())
                    == Some(e.uid.as_str());
            for t in tracks {
                let clock = if !hud_state.tactical {
                    self.working_playback.clock(scene, e, t, clock)
                } else {
                    clock
                };
                for s in t.frame(clock) {
                    draws += self
                        .sale_sprite(canvas, camera, &t.texture, s.src, s.rect, p, sale_red)?
                        as usize;
                }
            }
            if let (Some(c), Some(pose)) = (&assets.combat, pose) {
                if e.kind != "unit" && pose.pose == "attack" {
                    if let Some(tracks) = c
                        .animations
                        .get(&format!("building/{}/{}/attack", e.type_id, pose.facing))
                    {
                        for track in tracks {
                            for sprite in track.frame(pose.time) {
                                draws += self.sprite(
                                    canvas,
                                    camera,
                                    &track.texture,
                                    sprite.src,
                                    sprite.rect,
                                    p,
                                )? as usize;
                            }
                        }
                    }
                }
            }
            if let (Some(c), Some((motion, elapsed))) = (&assets.combat, hud_state.motion) {
                for hit in motion
                    .hits
                    .iter()
                    .filter(|h| h.target == e.uid && h.start <= elapsed && elapsed < h.end)
                {
                    if let Some(clip) = c.hit_clips.get(&hit.clip) {
                        // Unit.animationCreate uses pivotY=-36; buildings attach
                        // the clip directly to pSprite without that unit offset.
                        let origin = [p[0], p[1] + if e.kind == "unit" { 36. } else { 0. }];
                        for track in &clip.tracks {
                            for sprite in track.frame(elapsed - hit.start) {
                                draws += self.sprite(
                                    canvas,
                                    camera,
                                    &track.texture,
                                    sprite.src,
                                    sprite.rect,
                                    origin,
                                )? as usize;
                            }
                        }
                    }
                }
            }
        }
        if let Some((e, valid)) = hud_state.preview {
            let [x, y, w, h] = e.footprint();
            let points = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
                .map(|p| camera.screen(scene.projection.project(p[0], p[1])));
            source_quad(
                canvas,
                points,
                if valid {
                    Color::RGBA(0, 255, 0, 179)
                } else {
                    Color::RGBA(255, 0, 0, 179)
                },
            )?;
            let p = scene.projection.project(e.x, e.y);
            for t in &e.tracks {
                for s in t.frame(warzone_app_client::animation::idle_time(&e.uid, seconds)) {
                    if !matches!(e.kind.as_str(), "unit" | "units") {
                        self.ensure_texture(&t.texture)?;
                        self.textures.get_mut(&t.texture).unwrap().set_color_mod(
                            if valid { 0 } else { 255 },
                            if valid { 255 } else { 0 },
                            0,
                        );
                    }
                    let result = self.sprite(canvas, camera, &t.texture, s.src, s.rect, p);
                    if !matches!(e.kind.as_str(), "unit" | "units") {
                        self.textures
                            .get_mut(&t.texture)
                            .unwrap()
                            .set_color_mod(255, 255, 255);
                    }
                    draws += result? as usize;
                }
            }
        }
        if hud_state.tactical {
            for (e, pose) in &entities {
                if e.kind != "unit" && e.type_id != 1500 {
                    continue;
                }
                if hud_state.original.is_some_and(|v| v.retained_unit_health) {
                    continue;
                }
                if hud_state.original.is_some() {
                    // SceneBattle.showUnitHealth on a hit, hideUnitHealth in
                    // onUnitFireFinished unless the victim remains selected.
                    // Idle full-health enemies do not expose debug bars.
                    let selected_unit = selected
                        .and_then(|i| scene.entities.get(i))
                        .is_some_and(|s| s.uid == e.uid);
                    let during_damage = pose
                        .as_ref()
                        .is_some_and(|p| p.hp != e.archived["hp"].as_f64().unwrap_or(0.));
                    if !selected_unit && !during_damage {
                        continue;
                    }
                }
                let hp = pose
                    .as_ref()
                    .map_or_else(|| e.archived["hp"].as_f64().unwrap_or(0.), |p| p.hp);
                let max = e.archived["hp_max"].as_f64().unwrap_or(1.);
                if hp <= 0. {
                    continue;
                }
                let (x, y) = pose.as_ref().map_or((e.x, e.y), |p| (p.x, p.y));
                let p = camera.screen(scene.projection.project(x, y));
                let x = p[0] as i32 - 20;
                let y = (p[1] - 35. * camera.zoom) as i32;
                canvas.set_draw_color(Color::RGB(15, 20, 15));
                canvas.fill_rect(Rect::new(x - 1, y - 1, 42, 7))?;
                canvas.set_draw_color(if e.archived["enemy"] == true {
                    Color::RGB(210, 70, 45)
                } else {
                    Color::RGB(115, 205, 40)
                });
                canvas.fill_rect(Rect::new(
                    x,
                    y,
                    (40. * (hp / max).clamp(0., 1.)).max(1.) as u32,
                    5,
                ))?;
            }
        }
        if background_capture {
            return Ok(draws);
        }
        if let Some(markers) = hud_state.world_markers {
            self.source_interface(canvas, markers)?;
        }
        canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
        if scene.hud.is_some() {
            self.original_hud(canvas, scene, camera, &hud_state.values)?;
        } else {
            canvas.set_draw_color(Color::RGBA(25, 32, 20, 230));
            canvas.fill_rect(Rect::new(0, 0, camera.width, 40))?;
            let r = hud_state.resources;
            self.fit_label(
                canvas,
                &format!(
                    "Ур. {}   Энергия {}   Монеты {}   Кэш {}   Нефть {}   Сталь {}",
                    r.get("level").unwrap_or(&0),
                    hud_state.energy,
                    r.get("playMoney").unwrap_or(&0),
                    r.get("realMoney").unwrap_or(&0),
                    r.get("fuel").unwrap_or(&0),
                    r.get("iron").unwrap_or(&0)
                ),
                Rect::new(16, 7, camera.width.saturating_sub(32), 28),
            )?;
        }
        if pause && size.0 as u64 * size.1 as u64 * 4 <= 32 * 1024 * 1024 {
            let rgba = read_rgba(canvas)?;
            let mut texture = self
                .creator
                .create_texture_static(PixelFormatEnum::RGBA32, size.0, size.1)
                .map_err(|e| e.to_string())?;
            texture
                .update(None, &rgba, size.0 as usize * 4)
                .map_err(|e| e.to_string())?;
            texture.set_blend_mode(sdl2::render::BlendMode::None);
            texture.set_scale_mode(sdl2::render::ScaleMode::Nearest);
            self.paused_world = Some(PausedWorld {
                size,
                camera: pose,
                texture,
            });
            self.paused_misses += 1;
        }
        if let Some(original) = hud_state.original {
            self.cached_source_interface(canvas, original)?;
        } else {
            let labels = hud_state.navigation.unwrap_or_else(|| {
                ["Строить · C", "Ресурсы · B", "Инвентарь · I", "Задания · Q"].map(String::from)
            });
            for (rect, label) in warzone_app_client::hud::navigation(scene, camera)
                .iter()
                .zip(labels)
            {
                let [x, y, w, h] = *rect;
                canvas.set_draw_color(Color::RGBA(25, 32, 20, 210));
                canvas.fill_rect(Rect::new(x + 2, y, w.saturating_sub(4) as u32, h as u32))?;
                self.fit_label(
                    canvas,
                    &label,
                    Rect::new(
                        x + 6,
                        y + 4,
                        w.saturating_sub(12) as u32,
                        h.saturating_sub(8) as u32,
                    ),
                )?;
            }
            let panel = if let Some(e) = hud_state
                .preview
                .map(|(e, _)| e)
                .or_else(|| selected.map(|i| &scene.entities[i]))
            {
                format!(
                    "{}   ·   ({}, {})",
                    scene.item_name(e.type_id, e.name.trim()),
                    e.x,
                    e.y
                )
            } else {
                if hud_state.tactical {
                    "ЛКМ: выбрать отряд · Перетаскивание: камера · Колесо: масштаб"
                } else {
                    "ЛКМ: выбрать · Перетаскивание: камера · Колесо: масштаб · Home: база"
                }
                .into()
            };
            canvas.set_draw_color(Color::RGBA(25, 32, 20, 230));
            canvas.fill_rect(Rect::new(0, camera.height as i32 - 138, camera.width, 138))?;
            self.label(canvas, &panel, 16, camera.height as i32 - 132)?;
            self.fit_label(
                canvas,
                hud_state.detail,
                Rect::new(
                    16,
                    camera.height as i32 - 104,
                    camera.width.saturating_sub(32),
                    28,
                ),
            )?;
            self.label(canvas, hud_state.message, 16, camera.height as i32 - 76)?;
            if !hud_state.action.is_empty() {
                self.button(
                    canvas,
                    scene,
                    Rect::new(16, camera.height as i32 - 44, 170, 36),
                    &format!("{} · Enter", hud_state.action),
                    true,
                )?;
                self.button(
                    canvas,
                    scene,
                    Rect::new(200, camera.height as i32 - 44, 230, 36),
                    if hud_state.editing {
                        "Отменить · Esc"
                    } else if hud_state.tactical {
                        "Отряды · Tab"
                    } else {
                        "Другой рецепт · Tab"
                    },
                    true,
                )?;
            }
            if selected.is_some() && !hud_state.editing && !hud_state.tactical {
                let x = camera.width as i32 - 320;
                let y = camera.height as i32 - 132;
                canvas.set_draw_color(Color::RGB(60, 70, 42));
                canvas.fill_rect(Rect::new(x, y, 150, 26))?;
                canvas.fill_rect(Rect::new(x + 156, y, 150, 26))?;
                self.label(canvas, "Перенести · M", x + 5, y + 2)?;
                self.label(canvas, "Повернуть · R", x + 161, y + 2)?;
                self.label(canvas, "Продать · Delete", x + 5, y + 30)?;
            }
        }
        if let Some(q) = hud_state.quests {
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 175));
            canvas.fill_rect(Rect::new(
                0,
                40,
                camera.width,
                camera.height.saturating_sub(40),
            ))?;
            let layout = modal_layout(camera);
            let width = layout.panel.width();
            let height = layout.panel.height();
            let x = layout.panel.x();
            let y = layout.panel.y();
            if let Some(p) = &scene.presentation {
                self.skin(canvas, &p.panel, layout.panel, 10)?;
                self.ink = 0x483919;
            } else {
                canvas.set_draw_color(Color::RGB(35, 45, 28));
                canvas.fill_rect(layout.panel)?;
            }
            let heading = if q.total > 1 {
                format!("{} · {}/{}", q.section, q.index, q.total)
            } else {
                q.section.clone()
            };
            if self.styled {
                self.fit_text(
                    canvas,
                    &heading,
                    Rect::new(x + 16, y + 5, width.saturating_sub(80), 38),
                    28.,
                    TITLE,
                    0xffd400,
                )?;
            } else {
                self.label(canvas, &heading, x + 20, y + 14)?;
            }
            self.label(canvas, &q.title, x + 20, y + 47)?;
            let mut row_y = y + 88;
            for row in q.rows {
                row_y =
                    self.wrapped_label(canvas, &row, x + 20, row_y, width.saturating_sub(40))? + 14;
            }
            self.wrapped_label(
                canvas,
                &q.awards,
                x + 20,
                y + height as i32 - 112,
                width.saturating_sub(40),
            )?;
            if let Some(p) = &scene.presentation {
                self.ensure_texture(&p.close)?;
                canvas.copy(&self.textures[&p.close], None, Some(layout.close))?;
            } else {
                canvas.set_draw_color(Color::RGB(65, 83, 43));
                canvas.fill_rect(layout.close)?;
                self.label(canvas, "×", layout.close.x() + 10, layout.close.y() + 2)?;
            }
            if !q.primary_label.is_empty() {
                self.button(canvas, scene, layout.claim, &q.primary_label, q.ready)?;
            }
            if q.total > 1 {
                self.button(canvas, scene, layout.previous, "Назад", true)?;
                self.button(canvas, scene, layout.next, "Далее", true)?;
            }
            self.ink = 0xf5f0d5;
        }
        Ok(draws)
    }
    fn source_button_layout(
        &mut self,
        view: &warzone_app_client::interface::View,
    ) -> Result<BTreeMap<usize, [f64; 4]>, String> {
        let mut frames = BTreeMap::new();
        // MTLabel.changeFontSize performs calcSize before setCenter. The
        // Russian challenge caption is a single complete line; an arbitrary
        // template width wraps "другом" below its eighteen-point frame.
        for (i, d) in view
            .draws
            .iter()
            .enumerate()
            .filter(|(_, d)| d.node.id == "gcBtn_txt")
        {
            let n = &d.node;
            let width = source_advance(
                &self.fonts[source_face(n)],
                &n.text,
                n.font_size as f32,
                n.letter_spacing,
            ) + 4.;
            let mut r = n.rect;
            r[0] += (r[2] - width) * 0.5;
            r[2] = width;
            frames.insert(i, r);
        }
        for (left_id, icon_id, right_id) in [
            ("refill_text", "cacheicon_img", "refill_value"),
            ("buy_for_txt", "money_image", "money_txt"),
            ("buy_for2_txt", "iconPlayMoney", "pricemoney_txt"),
            ("ok_txt", "money_img", "buyprice_txt"),
            ("unlockbuy_txt", "unlockicon_img", "unlockprice_txt"),
            (
                "researchlabel_txt",
                "researchclock_Image",
                "researchtime_txt",
            ),
            ("item_buy_txt", "cache_img2", "item_price_txt"),
            ("accesslabel1_txt", "cache_img", "unlock1_txt"),
            ("buybtn_txt", "realMoney_icon", "buyprice_txt"),
            ("button_txt", "unlock_cache_img", "btn_real_money_txt"),
            ("unlockname_txt", "money_img", "unlockprice_txt"),
            ("upgradelabel_txt", "clock_image", "upgradetime_txt"),
            ("item_buy_txt", "item_cache2_img", "item_price_txt"),
        ] {
            for (li, left) in view
                .draws
                .iter()
                .enumerate()
                .filter(|(_, d)| d.node.id == left_id)
            {
                let left_path = source_control_path(&left.node);
                let prefix = &left_path[..left_path.len().saturating_sub(1)];
                let same = |n: &warzone_app_client::interface::Node| {
                    let path = source_control_path(n);
                    path.len() == prefix.len() + 1 && path.starts_with(prefix)
                };
                let Some((ri, right)) = view
                    .draws
                    .iter()
                    .enumerate()
                    .find(|(_, d)| d.node.id == right_id && same(&d.node))
                else {
                    continue;
                };
                let Some((ii, icon)) = view
                    .draws
                    .iter()
                    .enumerate()
                    .find(|(_, d)| d.node.id == icon_id && same(&d.node))
                else {
                    continue;
                };
                let Some(bg) = view.draws.iter().find(|d| {
                    let ancestry = source_control_path(&d.node);
                    let path = ancestry.into_iter().filter(|p| {
                        !matches!(*p, "source_button_background" | "source_button_pressed")
                    });
                    d.node.kind == "image"
                        && if left_id == "ok_txt" {
                            path.eq(["ok_btn"])
                        } else if left_id == "buybtn_txt" {
                            path.eq(["speedup_btn"])
                        } else {
                            path.eq(prefix.iter().copied())
                        }
                }) else {
                    continue;
                };
                // PKUtils measures CTLine advance, excluding the offscreen
                // raster's shadow/glow padding. Padding is never layout width.
                let width = |n: &warzone_app_client::interface::Node| -> Result<f64, String> {
                    Ok(source_advance(
                        &self.fonts[source_face(n)],
                        &n.text,
                        n.font_size as f32,
                        n.letter_spacing,
                    ))
                };
                let lw = width(&left.node)?;
                let rw = width(&right.node)?;
                let iw = icon.node.rect[2];
                if left_id == "refill_text" {
                    let margin = 4. * bg.node.rect[3] / 33.;
                    let x = bg.node.rect[0] + (bg.node.rect[2] - lw - iw - rw - 2. * margin) * 0.5;
                    let mut l = left.node.rect;
                    l[0] = x;
                    l[2] = lw + 0.5;
                    frames.insert(li, l);
                    let mut i = icon.node.rect;
                    i[0] = x + lw + margin;
                    frames.insert(ii, i);
                    let mut r = right.node.rect;
                    r[0] = i[0] + iw + margin;
                    r[2] = rw + 0.5;
                    frames.insert(ri, r);
                    continue;
                }
                if left_id == "buybtn_txt" {
                    let [lx, lw, ix, _, rx, rw] = speedup_group_frames(bg.node.rect, lw, iw, rw);
                    let mut l = left.node.rect;
                    l[0] = lx;
                    // PKUtils.setPos moves the UILabel without shrinking its
                    // authored frame. Its centred ink retains that padding.
                    l[2] = l[2].max(lw + 0.5);
                    frames.insert(li, l);
                    let mut r = right.node.rect;
                    r[0] = rx;
                    r[2] = r[2].max(rw + 0.5);
                    frames.insert(ri, r);
                    let mut i = icon.node.rect;
                    i[0] = ix;
                    frames.insert(ii, i);
                    continue;
                }
                let mut lx = (bg.node.rect[0] + (bg.node.rect[2] - lw - iw - rw) * 0.5).trunc();
                let ix = lx + lw;
                let mut rx = ix + iw;
                // UIButton2's default outer margin is2 source points. Its
                // overflow branch pins text to the inset rectangle edges.
                let scale = bg.node.rect[3]
                    / match left_id {
                        "item_buy_txt" => 28.,
                        "accesslabel1_txt" => 30.,
                        _ => 40.,
                    };
                if lw + iw + rw > bg.node.rect[2] - 14. * scale {
                    lx = bg.node.rect[0] + 7. * scale;
                    rx = bg.node.rect[0] + bg.node.rect[2] - 7. * scale - rw;
                }
                let mut l = left.node.rect;
                l[0] = lx;
                l[2] = if left_id == "button_txt" {
                    l[2].max(lw + 0.5)
                } else {
                    lw + 0.5
                };
                frames.insert(li, l);
                let mut r = right.node.rect;
                r[0] = rx;
                r[2] = if left_id == "button_txt" {
                    r[2].max(rw + 0.5)
                } else {
                    rw + 0.5
                };
                frames.insert(ri, r);
                let mut image = icon.node.rect;
                image[0] = ix;
                frames.insert(ii, image);
            }
        }
        Ok(frames)
    }
    pub fn source_button_report(
        &mut self,
        view: Option<&warzone_app_client::interface::View>,
    ) -> Result<serde_json::Value, String> {
        let Some(view) = view else {
            return Ok(serde_json::Value::Null);
        };
        let frames = self.source_button_layout(view)?;
        Ok(
            serde_json::json!({"drawn":self.source_button_trace,"layout":view.draws.iter().enumerate().filter(|(_,d)|["button_txt","btn_real_money_txt","play_money_txt"].contains(&d.node.id.as_str())).map(|(i,d)|serde_json::json!({"id":d.node.id,"text":d.node.text,"parents":d.node.parents,"source_rect":d.node.rect,"render_rect":frames.get(&i).copied().unwrap_or(d.node.rect),"clip":d.clip})).collect::<Vec<_>>()}),
        )
    }
    fn source_lines(&self, n: &warzone_app_client::interface::Node, w: f64) -> Vec<String> {
        let face = source_face(n);
        if n.parents.iter().any(|p| p == "source_falling_count")
            || matches!(n.id.as_str(), "buybtn_txt" | "gcBtn_txt")
        {
            return vec![n.text.clone()];
        }
        let key = format!(
            "{face}:{}:{}:{}:{}",
            n.font_size.to_bits(),
            w.to_bits(),
            n.letter_spacing.to_bits(),
            n.text
        );
        if let Some(lines) = self.line_layouts.borrow().get(&key) {
            return lines.clone();
        }
        // Advance and wrapping must share the same arithmetic. Previously a
        // float32/float64 disagreement wrapped the final price digit by <1px.
        let lines = wrap_source_text(&n.text, true, w + 0.25, |text| {
            source_advance(
                &self.fonts[face],
                text,
                n.font_size as f32,
                n.letter_spacing,
            )
        });
        let mut cache = self.line_layouts.borrow_mut();
        if cache.len() >= 1024 {
            cache.clear();
        }
        cache.insert(key, lines.clone());
        lines
    }
    fn source_story_frames(
        &self,
        view: &warzone_app_client::interface::View,
        frames: &mut BTreeMap<usize, [f64; 4]>,
    ) {
        // Unit.createTooltipMini uses the actual caption font's textHeight.
        // Candara metrics can spuriously wrap this Franklin caption and push
        // it much too far above the health bar.
        for (i, d) in view.draws.iter().enumerate().filter(|(_, d)| {
            d.node.id == "caption_txt" && d.node.parents.iter().any(|p| p == "source_unit_health")
        }) {
            let n = &d.node;
            let scale = n
                .parents
                .iter()
                .find_map(|p| p.strip_prefix("source_label_scale_")?.parse::<f64>().ok())
                .unwrap_or(1.);
            let height =
                self.source_lines(n, n.rect[2]).len().max(1) as f64 * n.line_height + 6. * scale;
            let mut rect = n.rect;
            rect[1] -= height - rect[3];
            rect[3] = height;
            frames.insert(i, rect);
        }
        let Some(hint) = view.draws.iter().find(|d| {
            d.node.id == "hint_txt" && d.node.parents.iter().any(|p| p == "source_story_balloon")
        }) else {
            return;
        };
        let n = &hint.node;
        let scale = n.font_size / 14.;
        let count = self.source_lines(n, n.rect[2]).len().max(1) as f64;
        let text_h = count * n.line_height;
        let tap_h = view
            .draws
            .iter()
            .find(|d| {
                d.node.id == "tap_txt" && d.node.parents.iter().any(|p| p == "source_story_balloon")
            })
            .map_or(0., |d| {
                d.node.line_height.max(d.node.font_size * 1.2) + 4. * scale
            });
        let height = view
            .draws
            .iter()
            .find(|d| d.node.kind == "shade")
            .map_or_else(
                || {
                    n.parents
                        .iter()
                        .find_map(|p| p.strip_prefix("story_height_")?.parse::<f64>().ok())
                        .unwrap_or(640.)
                },
                |d| d.node.rect[3],
            );
        let head = n
            .parents
            .iter()
            .find_map(|p| p.strip_prefix("story_head_")?.parse::<f64>().ok())
            .unwrap_or(0.);
        let base = n.rect[1] - 13.5 * scale;
        let top = base + height - (270. - head) * scale;
        let total = if tap_h == 0. {
            26. * scale + text_h
        } else {
            13.5 * scale + text_h + tap_h + 21. * scale
        };
        for (i, d) in view
            .draws
            .iter()
            .enumerate()
            .filter(|(_, d)| d.node.parents.iter().any(|p| p == "source_story_balloon"))
        {
            let mut r = d.node.rect;
            match d.node.id.as_str() {
                "bubble_top" => r[1] = top,
                "bubble_mdl" => {
                    r[1] = top + 10. * scale;
                    r[3] = total - 16. * scale;
                }
                "bubble_btm" => r[1] = top + total - 10. * scale,
                "hint_txt" => {
                    r[1] = top + 13.5 * scale;
                    r[3] = text_h;
                }
                "tap_txt" => {
                    r[1] = top + total - 12.5 * scale - tap_h;
                    r[3] = tap_h;
                }
                "arrowLeft" | "arrowRight" => r[1] = top + 25. * scale - r[3] / 2.,
                _ => continue,
            }
            frames.insert(i, r);
        }
    }
    pub fn loading(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        camera: &Camera,
        language: u32,
    ) -> Result<bool, String> {
        let Some(c) = &scene.interface else {
            return Ok(false);
        };
        let scale = warzone_app_client::hud::scale(camera);
        let key = if camera.width as f64 / scale > 480. {
            "progressScreen_long"
        } else {
            "progressScreen"
        };
        let Some(root) = c.symbols.get(key) else {
            return Ok(false);
        };
        let origin = [
            (camera.width as f64 / scale - root.size[0]) * 0.5,
            (camera.height as f64 / scale - root.size[1]) * 0.5,
        ];
        let mut v = warzone_app_client::interface::View::default();
        v.add(
            root,
            origin,
            scale,
            |n| {
                !n.parents
                    .iter()
                    .any(|p| p == if language == 1 { "logo_en" } else { "logo_ru" })
            },
            |n| match n.id.as_str() {
                "status_txt" => scene
                    .progression
                    .as_ref()
                    .and_then(|c| c.language.get("#loading_map"))
                    .cloned(),
                "percents_txt" => Some("0%".into()),
                _ => None,
            },
            None,
        );
        for d in &mut v.draws {
            if d.node.kind == "image"
                && d.node.parents.is_empty()
                && d.node.rect[2] >= root.size[0] * scale
            {
                // Fill the viewport with the source artwork, preserving its
                // aspect ratio; the logo/progress retain their readable size.
                let cover =
                    (camera.width as f64 / root.size[0]).max(camera.height as f64 / root.size[1]);
                d.node.rect = [
                    (camera.width as f64 - root.size[0] * cover) / 2.,
                    (camera.height as f64 - root.size[1] * cover) / 2.,
                    root.size[0] * cover,
                    root.size[1] * cover,
                ];
            }
            if d.node.id == "bar" {
                d.progress = 0.;
            }
        }
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        self.source_interface(canvas, &v)?;
        Ok(true)
    }
    /// Decode/rasterize the common sheets while the HTML loading screen still
    /// covers the canvas. Opening them must not compile/rasterize every label
    /// in response to the click. LRU byte/count budgets continue to apply.
    #[cfg(target_os = "emscripten")]
    pub fn warm_menus(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &Scene,
        state: &warzone_app_client::production::State,
        camera: &Camera,
    ) -> Result<(), String> {
        use warzone_app_client::interface::{self, Screen, View};
        for screen in [Screen::Inventory, Screen::Settings] {
            if let Some(view) =
                interface::build(scene, state, camera, Some(screen), None, false, 0., false)
            {
                self.source_interface(canvas, &view)?;
            }
        }
        for panel in ["achievements", "leagues", "pvp"] {
            let mut view = View::default();
            if panel == "achievements" {
                warzone_app_client::interface_achievements::build(
                    &mut view, scene, state, camera, 0.,
                );
            } else if panel == "leagues" {
                warzone_app_client::interface_leagues::build(&mut view, scene, state, camera, 0);
            } else {
                warzone_app_client::interface_pvp::profile(&mut view, scene, state, camera);
            }
            self.source_interface(canvas, &view)?;
        }
        // Page artwork and localized labels are stable during a swipe. Prepare
        // them once under the loader instead of decoding on the first gesture.
        if let Some(width) = warzone_app_client::interface_leagues::page_width(scene, camera) {
            for page in 0..5 {
                let mut view = View::default();
                warzone_app_client::interface_leagues::help(
                    &mut view,
                    scene,
                    state,
                    camera,
                    page as f64 * width,
                );
                self.source_interface(canvas, &view)?;
            }
        }
        self.warm_ui.extend(self.textures.keys().cloned());
        canvas.set_clip_rect(None);
        Ok(())
    }
    fn source_interface(
        &mut self,
        canvas: &mut Canvas<Window>,
        view: &warzone_app_client::interface::View,
    ) -> Result<(), String> {
        self.source_interface_layer(canvas, view, false)
    }
    fn cached_source_interface(
        &mut self,
        canvas: &mut Canvas<Window>,
        view: &warzone_app_client::interface::View,
    ) -> Result<(), String> {
        let size = canvas.output_size()?;
        if !self.cache_source_views
            || view.scrolling
            || size.0 as u64 * size.1 as u64 * 4 > 32 * 1024 * 1024
            || view
                .draws
                .iter()
                .any(|d| d.node.id == "source_tutorial_arrow")
        {
            return self.source_interface(canvas, view);
        }
        // Serialize the exact rendered inputs, not revision/time alone: clips,
        // tween geometry, progress, opacity, live labels and locale all matter.
        self.source_view_key.clear();
        serde_json::to_writer(
            &mut self.source_view_key,
            &(&view.draws, &view.extra_tutorial_holes),
        )
        .map_err(|e| e.to_string())?;
        let mut cached = self.source_view.take().filter(|old| old.size == size);
        if let Some(old) = &cached {
            if old.key == self.source_view_key {
                self.source_view_hits += 1;
                if self.trace_source_buttons {
                    self.source_button_trace = old.trace.clone();
                }
                canvas.copy(&old.texture, None, None)?;
                self.source_view = cached;
                return Ok(());
            }
        }
        if cached.is_none() {
            let Ok(mut texture) =
                self.creator
                    .create_texture_target(PixelFormatEnum::RGBA32, size.0, size.1)
            else {
                return self.source_interface(canvas, view);
            };
            // An RGBA render target holds premultiplied accumulated colors.
            // Software's straight-alpha fallback therefore keeps direct draws.
            if !source_premultiplied_blend(&mut texture) {
                return self.source_interface(canvas, view);
            }
            texture.set_scale_mode(sdl2::render::ScaleMode::Nearest);
            cached = Some(SourceViewCache {
                key: Vec::new(),
                size,
                texture,
                trace: Vec::new(),
            });
        }
        let mut cached = cached.unwrap();
        let mut rendered = Ok(());
        let previous = canvas.clip_rect();
        source_clip(canvas, None)?;
        canvas
            .with_texture_canvas(&mut cached.texture, |target| {
                target.set_draw_color(Color::RGBA(0, 0, 0, 0));
                target.clear();
                rendered = self.source_interface_layer(target, view, false);
            })
            .map_err(|e| e.to_string())?;
        source_clip(canvas, previous)?;
        rendered?;
        cached.key.clone_from(&self.source_view_key);
        cached.trace = self.source_button_trace.clone();
        self.source_view_misses += 1;
        canvas.copy(&cached.texture, None, None)?;
        self.source_view = Some(cached);
        Ok(())
    }
    fn source_interface_layer(
        &mut self,
        canvas: &mut Canvas<Window>,
        view: &warzone_app_client::interface::View,
        under: bool,
    ) -> Result<(), String> {
        self.source_button_trace.clear();
        let previous = canvas.clip_rect();
        let size = canvas.output_size()?;
        let viewport = Rect::new(0, 0, size.0, size.1);
        let mut button_frames = self.source_button_layout(view)?;
        self.source_story_frames(view, &mut button_frames);
        for (index, draw) in view.draws.iter().enumerate() {
            let fitted = if draw.node.kind == "text" {
                fitting_cash_label(&draw.node, &self.fonts[source_face(&draw.node)])
            } else {
                None
            };
            let n = fitted.as_ref().unwrap_or(&draw.node);
            if n.parents.iter().any(|p| p == "battle_underlay") != under {
                continue;
            }
            let [x, y, w, h] = button_frames.get(&index).copied().unwrap_or(n.rect);
            let clip = draw.clip.map(|[x, y, w, h]| {
                Rect::new(
                    x.floor() as i32,
                    y.floor() as i32,
                    w.ceil().max(1.) as u32,
                    h.ceil().max(1.) as u32,
                )
            });
            if clip.is_some_and(|c| c.intersection(viewport).is_none()) {
                continue;
            }
            if matches!(n.kind.as_str(), "text" | "image")
                && clip.is_some_and(|c| {
                    c.intersection(Rect::new(
                        x.floor() as i32,
                        y.floor() as i32,
                        w.ceil().max(1.) as u32,
                        h.ceil().max(1.) as u32,
                    ))
                    .is_none()
                })
            {
                continue;
            }
            if n.kind == "deployment_tile" {
                source_clip(canvas, clip)?;
                source_quad(
                    canvas,
                    [
                        [x + w / 2., y],
                        [x + w, y + h / 2.],
                        [x + w / 2., y + h],
                        [x, y + h / 2.],
                    ],
                    Color::RGBA(
                        (n.color >> 16) as u8,
                        (n.color >> 8) as u8,
                        n.color as u8,
                        draw.opacity,
                    ),
                )?;
                continue;
            }
            if n.kind == "tutorial_mask" {
                source_clip(canvas, clip)?;
                let (sw, sh) = canvas.output_size()?;
                let key = format!(
                    "source_tutorial_mask:{sw}:{sh}:{x:.2}:{y:.2}:{w:.2}:{h:.2}:{}:{:?}",
                    draw.opacity, view.extra_tutorial_holes
                );
                if !self.text.contains_key(&key) {
                    // A moving hole used to leave one full-window RGBA texture
                    // per position in the label cache. Keep only the current
                    // mask; source text and unrelated small glyphs stay cached.
                    self.text
                        .retain(|k, _| !k.starts_with("source_tutorial_mask:"));
                    let mut rgba = vec![0u8; sw as usize * sh as usize * 4];
                    for py in 0..sh {
                        for px in 0..sw {
                            let coverage = |[x, y, w, h]: [f64; 4]| {
                                let d = (((px as f64 + 0.5 - x - w / 2.) / (w / 2.)).powi(2)
                                    + ((py as f64 + 0.5 - y - h / 2.) / (h / 2.)).powi(2))
                                .sqrt();
                                ((d - 1.) * h / 2. + 0.5).clamp(0., 1.)
                            };
                            let alpha = view
                                .extra_tutorial_holes
                                .iter()
                                .fold(coverage([x, y, w, h]), |a, r| a.min(coverage(*r)));
                            rgba[((py * sw + px) * 4 + 3) as usize] =
                                (alpha * draw.opacity as f64).round() as u8;
                        }
                    }
                    let mut t = self
                        .creator
                        .create_texture_static(PixelFormatEnum::RGBA32, sw, sh)
                        .map_err(|e| e.to_string())?;
                    t.update(None, &rgba, sw as usize * 4)
                        .map_err(|e| e.to_string())?;
                    t.set_blend_mode(sdl2::render::BlendMode::Blend);
                    self.text.insert(key.clone(), t);
                }
                canvas.copy(&self.text[&key], None, None)?;
                continue;
            }
            if n.kind == "tutorial_ring" {
                source_clip(canvas, clip)?;
                let sw = w.ceil().max(1.) as u32;
                let sh = h.ceil().max(1.) as u32;
                let key = format!("source_tutorial_ring:{sw}:{sh}");
                if !self.text.contains_key(&key) {
                    let mut rgba = vec![0u8; sw as usize * sh as usize * 4];
                    let rx = w / 2.;
                    let ry = h / 2.;
                    for py in 0..sh {
                        for px in 0..sw {
                            let dx = px as f64 + 0.5 - rx;
                            let dy = py as f64 + 0.5 - ry;
                            let implicit = (dx / rx).powi(2) + (dy / ry).powi(2) - 1.;
                            let grad =
                                2. * ((dx / rx.powi(2)).powi(2) + (dy / ry.powi(2)).powi(2)).sqrt();
                            let distance = implicit / grad.max(1e-9);
                            let alpha = ((distance + 6.5).clamp(0., 1.)
                                * (0.5 - distance).clamp(0., 1.)
                                * 255.)
                                .round() as u8;
                            let i = ((py * sw + px) * 4) as usize;
                            rgba[i] = 255;
                            rgba[i + 3] = alpha;
                        }
                    }
                    let mut t = self
                        .creator
                        .create_texture_static(PixelFormatEnum::RGBA32, sw, sh)
                        .map_err(|e| e.to_string())?;
                    t.update(None, &rgba, sw as usize * 4)
                        .map_err(|e| e.to_string())?;
                    t.set_blend_mode(sdl2::render::BlendMode::Blend);
                    t.set_scale_mode(sdl2::render::ScaleMode::Linear);
                    self.text.insert(key.clone(), t);
                }
                canvas.copy_f(
                    &self.text[&key],
                    None,
                    Some(FRect::new(x as f32, y as f32, w as f32, h as f32)),
                )?;
                continue;
            }
            if n.kind == "solid" {
                source_clip(canvas, clip)?;
                canvas.set_draw_color(Color::RGBA(
                    (n.color >> 16) as u8,
                    (n.color >> 8) as u8,
                    n.color as u8,
                    draw.opacity,
                ));
                canvas.fill_rect(Rect::new(x as i32, y as i32, w as u32, h as u32))?;
                continue;
            } else if n.kind == "shade" {
                source_clip(canvas, clip)?;
                canvas.set_draw_color(Color::RGBA(0, 0, 0, draw.opacity));
                canvas.fill_rect(Rect::new(x as i32, y as i32, w as u32, h as u32))?;
                continue;
            }
            if n.kind == "image" {
                let rw = (w * draw.progress).max(0.);
                let (sin, cos) = n.rotation.to_radians().sin_cos();
                let hx = cos.abs() * rw / 2. + sin.abs() * h / 2. + 1.;
                let hy = sin.abs() * rw / 2. + cos.abs() * h / 2. + 1.;
                let cx = x + rw / 2.;
                let cy = y + h / 2.;
                if cx + hx < 0.
                    || cy + hy < 0.
                    || cx - hx >= size.0 as f64
                    || cy - hy >= size.1 as f64
                {
                    continue;
                }
                source_clip(canvas, clip)?;
                let key =
                    if !draw.grayscale && n.parents.iter().any(|p| p == "source_button_pressed") {
                        format!("#source_highlight:{}", n.texture)
                    } else if draw.grayscale {
                        format!("#gray:{}", n.texture)
                    } else {
                        n.texture.clone()
                    };
                self.ensure_texture_pixels(&key, &n.texture, draw.grayscale)?;
                let t = &self.textures[&key];
                let q = t.query();
                let p = draw.progress;
                let source = n.source.unwrap_or([0, 0, q.width, q.height]);
                let t = self.textures.get_mut(&key).unwrap();
                t.set_alpha_mod(draw.opacity);
                if self.premultiplied.contains(&key) {
                    t.set_color_mod(draw.opacity, draw.opacity, draw.opacity);
                }
                if p > 0. {
                    canvas.copy_ex_f(
                        t,
                        Some(Rect::new(
                            source[0] as i32,
                            source[1] as i32,
                            (source[2] as f64 * p).round().max(1.) as u32,
                            source[3],
                        )),
                        Some(FRect::new(x as f32, y as f32, (w * p) as f32, h as f32)),
                        n.rotation,
                        None,
                        n.flip_x,
                        false,
                    )?;
                }
                t.set_alpha_mod(255);
                t.set_color_mod(255, 255, 255);
                continue;
            }
            let face = source_face(n);
            let lines = self.source_lines(n, w);
            let message_shift = n
                .parents
                .iter()
                .any(|p| p == "source_message_caption")
                .then(|| {
                    let scale = n.font_size / 14.;
                    ((h / scale - 14. - lines.len() as f64 * n.line_height / scale) * 0.5).round()
                        * scale
                });
            let text_clip = Rect::new(
                (x - 2.).floor() as i32,
                (y + message_shift.unwrap_or(0.)).floor() as i32,
                (w + 4.).ceil().max(1.) as u32,
                if message_shift.is_some() {
                    h.max(lines.len() as f64 * n.line_height)
                } else {
                    h
                }
                .ceil()
                .max(1.) as u32,
            );
            // FallingItem text is a separate world child, not a scroll-field.
            // Allow long quantities and their authored glow outside its template.
            let text_clip = if n.parents.iter().any(|p| p == "source_falling_count") {
                viewport
            } else {
                text_clip
            };
            let text_clip = if let Some(clip) = clip {
                let Some(r) = clip.intersection(text_clip) else {
                    continue;
                };
                r
            } else {
                text_clip
            };
            if text_clip.intersection(viewport).is_none() {
                continue;
            }
            // There is no draw between the inherited clip and this final text
            // clip. Change directly once; an intermediate reset doubled GL
            // flushes for every source label, including offscreen map markers.
            source_clip(canvas, Some(text_clip))?;
            // ShopDialog.showSelection moves one-line captions by+5 phone
            // points, and two-line troop captions by-2.
            let shop_caption =
                n.id == "caption_txt" && n.parents.iter().any(|p| p.starts_with("shop_item_"));
            // Use the rendered line count for the laboratory's title inset.
            let lab_caption = n.id == "caption_txt"
                && n.parents
                    .iter()
                    .any(|p| p.starts_with("source_lab_recipe_"));
            let shop_unit = shop_caption
                && n.parents.iter().any(|p| {
                    p.strip_prefix("shop_item_")
                        .and_then(|p| p.parse::<u32>().ok())
                        .is_some_and(|id| (2000..3000).contains(&id))
                });
            let caption_shift = if let Some(shift) = message_shift {
                // MessageBoxDialog.updateText: round((frame.height - 14 -
                // MTLabel._textHeight)/2); MTLabel accumulates its lineHeight.
                // Negative offsets are intentional. Clamping to0 clipped the
                // last line of the original first-star PvP explanation.
                shift
            } else if n.parents.iter().any(|p| p == "source_goals_hint") {
                // MissionGoalsDialog centers MTLabel._textHeight inside its
                // saved hintFrame after calcSize, without changing line height.
                ((h - lines.len() as f64 * n.line_height) * 0.5).round()
            } else if (shop_caption || lab_caption) && lines.len() == 1 {
                5. * n.font_size / 14.
            } else if shop_unit && lines.len() == 2 {
                -2. * n.font_size / 14.
            } else {
                0.
            };
            for (i, line) in lines.iter().enumerate() {
                let margin = n
                    .parents
                    .iter()
                    .find_map(|p| p.strip_prefix("source_top_margin_")?.parse::<f64>().ok())
                    .unwrap_or(0.);
                let yy = y + caption_shift + margin + i as f64 * n.line_height;
                if yy >= y + h {
                    break;
                }
                let key =
                    self.cache_styled_label(line, n.font_size as f32, face, n.color, Some(n))?;
                let t = self.text.get_mut(&key).unwrap();
                t.set_alpha_mod(draw.opacity);
                let q = t.query();
                let advance = source_advance(
                    &self.fonts[face],
                    line,
                    n.font_size as f32,
                    n.letter_spacing,
                );
                let pad = source_label_pad(n.font_size as f32, face, Some(n)) as f64;
                let xx = x + match n.align.as_str() {
                    "center" => (w - advance) * 0.5,
                    "right" => w - advance,
                    _ => 0.,
                } - pad
                    + n.parents
                        .iter()
                        .find_map(|p| p.strip_prefix("source_label_scale_")?.parse::<f64>().ok())
                        .unwrap_or(1.)
                        * match n.align.as_str() {
                            "center" => 0.5,
                            "right" => -1.,
                            _ => 1.,
                        };
                // MTLabel draws its baseline at1+UIFont.ascender. The raster
                // reserves effect padding around that glyph, not inside its frame.
                let yy = yy + 1. - pad;
                // Copy the raster 1:1 on whole output pixels. SDL's inverted
                // OpenGL render target can sample different glyph rows at a
                // half-pixel origin than the direct draw during scrolling.
                let target = Rect::new(xx.round() as i32, yy.round() as i32, q.width, q.height);
                if self.trace_source_buttons && n.id == "button_txt" {
                    self.source_button_trace.push(serde_json::json!({"parents":n.parents,"text":line,"target":[target.x(),target.y(),target.width() as i32,target.height() as i32],"clip":[text_clip.x(),text_clip.y(),text_clip.width(),text_clip.height()],"raster":[q.width,q.height],"color":n.color}));
                }
                canvas.copy(t, None, Some(target))?;
                t.set_alpha_mod(255);
            }
        }
        source_clip(canvas, previous)?;
        Ok(())
    }
    fn wrapped_label(
        &mut self,
        canvas: &mut Canvas<Window>,
        text: &str,
        x: i32,
        mut y: i32,
        width: u32,
    ) -> Result<i32, String> {
        let mut line = String::new();
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            // Measure with the same rasterization scale as label(), avoiding
            // split UTF-8 characters and hardcoded ASCII column widths.
            let advance: f32 = self.fonts[BODY]
                .layout(&candidate, Scale::uniform(20.), point(0., 0.))
                .map(|g| g.unpositioned().h_metrics().advance_width)
                .sum();
            if advance > width as f32 && !line.is_empty() {
                self.label(canvas, &line, x, y)?;
                y += 26;
                line = word.into();
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            self.label(canvas, &line, x, y)?;
            y += 26;
        }
        Ok(y)
    }
}

fn read_rgba(canvas: &Canvas<Window>) -> Result<Vec<u8>, String> {
    let (w, h) = canvas.output_size()?;
    let len = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= 256 * 1024 * 1024)
        .ok_or("Readback exceeds256MiB budget")?;
    // SDL may clip a readback while an X11 drawable is being resized. The
    // pinned Rust binding allocates uninitialized memory and then sets its full
    // length even if SDL wrote only the old rectangle. Keep every byte defined.
    let mut pixels = vec![0; len];
    let ret = unsafe {
        sdl2::sys::SDL_RenderReadPixels(
            canvas.raw(),
            std::ptr::null(),
            PixelFormatEnum::RGBA32 as u32,
            pixels.as_mut_ptr().cast(),
            (w * 4) as i32,
        )
    };
    if ret != 0 {
        return Err(sdl2::get_error());
    }
    Ok(pixels)
}

pub fn screenshot(canvas: &Canvas<Window>, path: &Path) -> Result<(), String> {
    let (w, h) = canvas.output_size()?;
    let rgba = read_rgba(canvas)?;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path).map_err(|e| e.to_string())?),
        w,
        h,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|e| e.to_string())?
        .write_image_data(&rgba)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod source_wrap_checks {
    use super::*;
    fn lines(text: &str, w: f64) -> Vec<String> {
        wrap_source_text(text, true, w, |s| s.chars().count() as f64)
    }
    #[test]
    fn source_mercenary_hyphen_break_is_preferred_to_a_trailing_two_letter_orphan() {
        assert_eq!(
            lines("Штурмовики-наёмники", 16.),
            ["Штурмовики-", "наёмники"]
        );
        assert_eq!(
            lines("a long-word suffix", 7.),
            ["a long-", "word", "suffix"]
        );
        assert_eq!(lines("long‐word", 5.), ["long‐", "word"]);
    }
    #[test]
    fn explicit_paragraphs_nonbreaking_space_and_unbroken_unicode_remain_intact() {
        assert_eq!(lines("Первый\n\nВторой", 30.), ["Первый", "", "Второй"]);
        assert_eq!(lines("Цена: 10\u{a0}руб.", 7.), ["Цена:", "10\u{a0}руб."]);
        assert_eq!(lines("Абвгдежз", 3.), ["Абв", "где", "жз"]);
        assert_eq!(
            wrap_source_text("Штурмовики-наёмники", false, 3., |s| s.len()
                as f64),
            ["Штурмовики-наёмники"]
        );
    }
}
