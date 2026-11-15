//! Shared MTLabel wrapping and source Candara measurements for dynamic quest rows.
use rusttype::{Font, Scale, point};
use std::sync::OnceLock;
pub fn wrap_source_text(
    text: &str,
    multiline: bool,
    width: f64,
    measure: impl Fn(&str) -> f64,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph
            .split(|c: char| c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{202f}'))
            .filter(|w| !w.is_empty())
        {
            let mut first = true;
            for fragment in word.split_inclusive(['-', '\u{2010}']) {
                let gap = if first && !line.is_empty() { " " } else { "" };
                let candidate = format!("{line}{gap}{fragment}");
                if multiline && measure(&candidate) > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                } else if !line.is_empty() {
                    line.push_str(gap);
                }
                if multiline && measure(fragment) > width {
                    for ch in fragment.chars() {
                        if !line.is_empty() && measure(&format!("{line}{ch}")) > width {
                            lines.push(std::mem::take(&mut line));
                        }
                        line.push(ch);
                    }
                } else {
                    line.push_str(fragment);
                }
                first = false;
            }
        }
        lines.push(line);
    }
    lines
}

fn candara(n: &crate::interface::Node) -> &'static Font<'static> {
    static F: OnceLock<[Font<'static>; 4]> = OnceLock::new();
    let faces = F.get_or_init(|| {
        [
            include_bytes!(concat!(env!("WARZONE_IPA_ASSETS"), "/CANDARA.TTF")).as_slice(),
            include_bytes!(concat!(env!("WARZONE_IPA_ASSETS"), "/CANDARAB.TTF")).as_slice(),
            include_bytes!(concat!(env!("WARZONE_IPA_ASSETS"), "/CANDARAI.TTF")).as_slice(),
            include_bytes!(concat!(env!("WARZONE_IPA_ASSETS"), "/CANDARAZ.TTF")).as_slice(),
        ]
        .map(|bytes| Font::try_from_bytes(bytes).expect("original Candara"))
    });
    &faces[n.bold as usize + 2 * n.italic as usize]
}
pub fn candara_height(n: &crate::interface::Node, text: &str) -> f64 {
    if text.is_empty() {
        return 0.;
    }
    let font = candara(n);
    let m = font.v_metrics_unscaled();
    let scale =
        Scale::uniform(n.font_size as f32 * (m.ascent - m.descent) / font.units_per_em() as f32);
    let lines = wrap_source_text(text, true, n.rect[2], |text| {
        let glyphs: Vec<_> = font.layout(text, scale, point(0., 0.)).collect();
        glyphs.last().map_or(0., |g| {
            g.position().x as f64 + g.unpositioned().h_metrics().advance_width as f64
        }) + text.chars().count().saturating_sub(1) as f64 * n.letter_spacing
    });
    lines.len() as f64 * n.line_height
}

/// QuestCompleteDialog calls MTLabel.calcSize before choosing its15/7pt offset.
pub fn title_lines(n: &crate::interface::Node, text: &str) -> usize {
    static TITLE: OnceLock<Font<'static>> = OnceLock::new();
    let font = TITLE.get_or_init(|| {
        Font::try_from_bytes(include_bytes!(concat!(
            env!("WARZONE_IPA_ASSETS"),
            "/a_DomIno BoldItalic.ttf"
        )))
        .expect("original title font")
    });
    let m = font.v_metrics_unscaled();
    let scale =
        Scale::uniform(n.font_size as f32 * (m.ascent - m.descent) / font.units_per_em() as f32);
    wrap_source_text(text, n.multiline, n.rect[2], |text| {
        let glyphs: Vec<_> = font.layout(text, scale, point(0., 0.)).collect();
        glyphs.last().map_or(0., |g| {
            g.position().x as f64 + g.unpositioned().h_metrics().advance_width as f64
        }) + text.chars().count().saturating_sub(1) as f64 * n.letter_spacing
    })
    .len()
}
