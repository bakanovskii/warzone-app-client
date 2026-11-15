//! LaboratoryDialog.updateContent0x1aca7d; original natural thumbnails/cost rows.
use crate::{
    Scene,
    interface::{self, Config, Draw, Symbol, View},
};
pub(crate) fn material_icon(c: &Config, id: u32) -> Option<&Symbol> {
    c.symbols
        .get(&format!("falling_item_{id}"))
        .or_else(|| c.item_icons.get(&id))
}
/// PKUtils.intervalToStringWithTimeParams:0x1e1a5 (minutes, hours+minutes, days).
pub(crate) fn recipe_time(scene: &Scene, seconds: i64) -> String {
    let minutes = (seconds / 60) % 60;
    let hours = seconds / 3600;
    let part = |key, fallback, n| {
        crate::interface_resources::number_text(interface::lang(scene, key, fallback), n)
    };
    let m = part("#minutes_short", "[1] мин.", minutes);
    if seconds < 3600 {
        return m;
    }
    let h = part("#hours_short", "[1] ч.", hours % 24);
    let mut result = if seconds < 86400 {
        h
    } else {
        format!(
            "{} {}.{}",
            seconds / 86400,
            interface::lang(scene, "#days_short2", "д").trim_end_matches('.'),
            h
        )
    };
    if minutes != 0 {
        result.push_str(&m);
    }
    result
}
pub(crate) fn card(template: &Symbol, r: &crate::workshop::Recipe) -> Symbol {
    let mut t = template.clone();
    let columns: Vec<_> = [
        (
            "play_money",
            r.cost.get("playMoney").copied().unwrap_or(0) > 0,
        ),
        ("oil", r.cost.get("fuel").copied().unwrap_or(0) > 0),
        ("iron", r.cost.get("iron").copied().unwrap_or(0) > 0),
        ("item1", !r.items.is_empty()),
        ("item2", r.items.len() > 1),
    ]
    .into_iter()
    .filter(|(_, yes)| *yes)
    .map(|(k, _)| k)
    .collect();
    let step = match columns.len() {
        2 => 30.,
        3 => 27.,
        _ => 25.,
    };
    for prefix in ["play_money", "oil", "iron", "item1", "item2"] {
        let named = |name: &str| name.starts_with(&format!("{prefix}_"));
        let Some(index) = columns.iter().position(|c| *c == prefix) else {
            t.nodes
                .retain(|n| !named(&n.id) && !n.parents.iter().any(|p| named(p)));
            continue;
        };
        let Some(rect) = template.frames.get(&format!("{prefix}_icon")) else {
            continue;
        };
        let x = 1. - (step * (columns.len() - 1) as f64 / 2.).trunc() + index as f64 * step;
        let dx = x - rect[0] - rect[2] / 2.;
        for n in &mut t.nodes {
            if named(&n.id) || n.parents.iter().any(|p| named(p)) {
                n.rect[0] += dx;
            }
        }
        for map in [&mut t.frames, &mut t.slots] {
            for (k, r) in map {
                if named(k) {
                    r[0] += dx;
                }
            }
        }
    }
    t
}
pub(crate) fn thumbnail(
    v: &mut View,
    c: &Config,
    scene: &Scene,
    id: u32,
    rect: [f64; 4],
    s: f64,
    clip: [f64; 4],
    gray: bool,
) -> bool {
    let Some(texture) = c.resource_icons.get(&id) else {
        return false;
    };
    let info = &scene.textures[texture];
    let (w, h) = (info.width as f64 / 2., info.height as f64 / 2.);
    let mut n = c.symbols["laboratory"].nodes[0].clone();
    n.kind = "image".into();
    n.id = format!("source_lab_thumb:{id}");
    n.texture = texture.clone();
    n.text.clear();
    n.parents = vec!["source_lab_thumbnail".into()];
    // UIImageView mode4=Center; original placeholder extent rounds down to even.
    n.rect = [
        (rect[0] + ((rect[2] as i64 & !1) as f64 - w) / 2.) * s,
        (rect[1] + ((rect[3] as i64 & !1) as f64 - h) / 2.) * s,
        w * s,
        h * s,
    ];
    v.draws.push(Draw {
        node: n,
        clip: Some(clip),
        progress: 1.,
        opacity: 255,
        grayscale: gray,
    });
    true
}
pub(crate) fn material(v: &mut View, c: &Config, id: u32, rect: [f64; 4], s: f64, clip: [f64; 4]) {
    let Some(icon) = material_icon(c, id) else {
        return;
    };
    let b = icon.nodes.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, n| {
            [
                b[0].min(n.rect[0]),
                b[1].min(n.rect[1]),
                b[2].max(n.rect[0] + n.rect[2]),
                b[3].max(n.rect[1] + n.rect[3]),
            ]
        },
    );
    let factor = 0.6;
    v.add(
        icon,
        [
            (rect[0] + rect[2] / 2.) / factor - (b[0] + b[2]) / 2.,
            (rect[1] + rect[3] / 2.) / factor - (b[1] + b[3]) / 2.,
        ],
        s * factor,
        |_| true,
        |_| None,
        Some(clip),
    );
}
