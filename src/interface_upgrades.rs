//! Academy retained source windows and original seven upgrade trees.
use crate::{
    Camera, Scene,
    interface::{self, Action, Draw, Symbol, View},
    production::State,
    upgrades::Upgrade,
};
use std::collections::BTreeMap;

/// UpgradeInfoDialog.updateGUIForSpecItem:0x15e4b1, presented over the academy.
pub fn material_info(v: &mut View, scene: &Scene, camera: &Camera, id: u32) {
    let Some(c) = &scene.interface else { return };
    let Some(template) = c.symbols.get("upgrade_info") else {
        return;
    };
    let mut root = template.clone();
    let title = scene.item_name(id, "").to_owned();
    let description = interface::lang(scene, &format!("#item_{id}_desc"), "").replace("\\n", "\n");
    let heights: [f64; 2] = ["header_txt", "description_txt"].map(|name| {
        let node = root.nodes.iter().find(|n| n.id == name).unwrap();
        crate::interface_text::candara_height(
            node,
            if name == "header_txt" {
                &title
            } else {
                &description
            },
        )
    });
    let first = 82. - ((heights[0] + heights[1] + 16.) as i64 / 2) as f64;
    for n in &mut root.nodes {
        if n.id == "header_txt" {
            n.rect[1] = first;
            n.rect[3] = heights[0] + 5.;
        }
        if n.id == "description_txt" {
            n.rect[1] = (first + heights[0] + 11.).trunc();
            n.rect[3] = heights[1] + 5.;
        }
    }
    let s = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / s - root.size[0]) / 2.,
        (camera.height as f64 / s - root.size[1]) / 2.,
    ];
    v.hits.clear();
    v.modal = true;
    v.popup_shade(&root.nodes[0], camera);
    v.add(
        &root,
        o,
        s,
        |n| !matches!(n.id.as_str(), "requiredheader_txt" | "requiredinfo_txt"),
        |n| {
            Some(match n.id.as_str() {
                "header_txt" => title.clone(),
                "description_txt" => description.clone(),
                _ => String::new(),
            })
        },
        None,
    );
    v.button(&root, "close_btn", o, s, Action::Close);
}

fn named(template: &Symbol, id: &str) -> Symbol {
    let mut t = template.clone();
    for n in &mut t.nodes {
        n.parents.insert(0, id.into());
    }
    t
}
fn picture(
    v: &mut View,
    scene: &Scene,
    t: &Symbol,
    texture: &str,
    frame: [f64; 4],
    o: [f64; 2],
    s: f64,
    clip: Option<[f64; 4]>,
    gray: bool,
) {
    let info = &scene.textures[texture];
    let (w, h) = (info.width as f64 * 0.5, info.height as f64 * 0.5);
    let mut n = t.nodes[0].clone();
    n.kind = "image".into();
    n.id = "source_upgrade_thumbnail".into();
    n.texture = texture.into();
    n.parents.clear();
    n.rect = [
        (o[0] + frame[0] + (frame[2] - w) * 0.5) * s,
        (o[1] + frame[1] + (frame[3] - h) * 0.5) * s,
        w * s,
        h * s,
    ];
    v.draws.push(Draw {
        node: n,
        clip,
        progress: 1.,
        opacity: 255,
        grayscale: gray,
    });
}
fn icon(v: &mut View, scene: &Scene, key: &str, center: [f64; 2], s: f64, clip: Option<[f64; 4]>) {
    let Some(t) = scene.interface.as_ref().and_then(|c| {
        if let Some(id) = key.strip_prefix("upgrade_small_") {
            c.work_icons.get(&id.parse().ok()?)
        } else {
            c.symbols.get(key)
        }
    }) else {
        return;
    };
    // GraphicsManager cancels the SP root pivot when constructing a UIView.
    let minx = t
        .nodes
        .iter()
        .map(|n| n.rect[0])
        .fold(f64::INFINITY, f64::min);
    let miny = t
        .nodes
        .iter()
        .map(|n| n.rect[1])
        .fold(f64::INFINITY, f64::min);
    let maxx = t
        .nodes
        .iter()
        .map(|n| n.rect[0] + n.rect[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let maxy = t
        .nodes
        .iter()
        .map(|n| n.rect[1] + n.rect[3])
        .fold(f64::NEG_INFINITY, f64::max);
    v.add(
        t,
        [
            center[0] - (minx + maxx) * 0.5,
            center[1] - (miny + maxy) * 0.5,
        ],
        s,
        |_| true,
        |_| None,
        clip,
    );
}
/// UpgradesTreeDialog.generatePathFrom:to:completed:option:0x15c181.
/// The source uses real card frames, original29pt dashes, circular junctions
/// and a directional arrow at the destination (including extended model cards).
fn draw_path(
    v: &mut View,
    c: &crate::interface::Config,
    color: &str,
    from: [f64; 2],
    to: [f64; 2],
    from_size: [f64; 2],
    to_size: [f64; 2],
    option: bool,
    s: f64,
    clip: [f64; 4],
) {
    let right = from[0] + from_size[0] / 2.;
    let left = to[0] - to_size[0] / 2.;
    let dy = to[1] - from[1];
    let (mut segments, arrow, arrow_at, corner) = if dy.abs() <= (left - right).abs() {
        (
            vec![([right, from[1]], [left - 31., to[1]], false)],
            "r",
            [left - 31., to[1]],
            None,
        )
    } else if option {
        let x = left + 50.;
        let y = if dy > 0. {
            to[1] - to_size[1] / 2. - 31.
        } else {
            to[1] + to_size[1] / 2.
        };
        (
            vec![
                ([right, from[1]], [x, from[1]], false),
                ([x, from[1]], [x, if dy > 0. { y } else { y + 31. }], true),
            ],
            if dy > 0. { "d" } else { "u" },
            [x, y],
            Some([x, from[1]]),
        )
    } else {
        let x = right - 50. - 12.;
        (
            vec![
                ([x, from[1]], [x, to[1]], true),
                ([x, to[1]], [left - 31., to[1]], false),
            ],
            "r",
            [left - 31., to[1]],
            Some([x, to[1]]),
        )
    };
    for (a, b, vertical) in segments.drain(..) {
        let axis = if vertical { 1 } else { 0 };
        let lo = a[axis].min(b[axis]);
        let hi = a[axis].max(b[axis]);
        if hi <= lo {
            continue;
        }
        let Some(t) = c.symbols.get(&format!(
            "upgrades_tree_{color}_dash_{}",
            if vertical { "v" } else { "h" }
        )) else {
            continue;
        };
        let piece = if vertical {
            [
                clip[0],
                (lo * s).max(clip[1]),
                clip[2],
                ((hi * s).min(clip[1] + clip[3]) - (lo * s).max(clip[1])).max(0.),
            ]
        } else {
            [
                (lo * s).max(clip[0]),
                clip[1],
                ((hi * s).min(clip[0] + clip[2]) - (lo * s).max(clip[0])).max(0.),
                clip[3],
            ]
        };
        for j in 0..((hi - lo) / 29.).ceil() as usize {
            let p = if vertical {
                [a[0], lo + j as f64 * 29.]
            } else {
                [lo + j as f64 * 29., a[1]]
            };
            v.add(t, p, s, |_| true, |_| None, Some(piece));
        }
    }
    if let Some(t) = c
        .symbols
        .get(&format!("upgrades_tree_{color}_arrow_{arrow}"))
    {
        let start = v.draws.len();
        v.add(
            t,
            arrow_at.map(f64::round),
            s,
            |_| true,
            |_| None,
            Some(clip),
        );
        for d in &mut v.draws[start..] {
            d.node.id = format!("source_upgrade_path_arrow_{arrow}");
        }
    }
    if let Some(p) = corner {
        if let Some(t) = c.symbols.get(&format!("upgrades_tree_{color}_circle")) {
            let start = v.draws.len();
            v.add(t, p.map(f64::round), s, |_| true, |_| None, Some(clip));
            for d in &mut v.draws[start..] {
                d.node.id = "source_upgrade_path_junction".into();
            }
        }
    }
}

pub fn description(scene: &Scene, u: &Upgrade) -> String {
    let mut rows = Vec::new();
    for m in &u.modifiers {
        if m.kind == "item" {
            for key in ["attack", "distance", "health", "speed", "population"] {
                if let Some(n) = m.values.get(key).and_then(|v| v.as_i64()) {
                    let label = interface::lang(scene, &format!("#{key}"), key);
                    rows.push(format!("{}: {n:+}", label.trim_end_matches(':')));
                }
            }
        }
        if m.kind == "recipe" {
            if let Some(n) = m.values.get("time").and_then(|v| v.as_i64()) {
                rows.push(format!(
                    "{}{} {}",
                    if n < 0 { "−" } else { "+" },
                    crate::interface_resources::duration(scene, n.abs()),
                    interface::lang(scene, "#production", "")
                ));
            }
            if let Some(n) = m.values.get("incoming").and_then(|v| v.as_f64()) {
                rows.push(format!(
                    "{} {}%",
                    interface::lang(scene, "#reduce_cost", ""),
                    (n * 100.).abs() as i64
                ));
            }
        }
    }
    rows.join("\n")
}
pub fn researched(scene: &Scene, state: &State, unit: u32) -> bool {
    scene
        .workshop
        .as_ref()
        .and_then(|w| w.recipes.get(&unit))
        .is_some_and(|r| state.research_ready(r.research))
}
pub fn build(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    unit: Option<u32>,
    detail: Option<u32>,
    info: bool,
    scroll: f64,
) -> bool {
    let (Some(c), Some(up)) = (&scene.interface, &scene.upgrades) else {
        return false;
    };
    if !c.symbols.contains_key("upgrades") {
        return false;
    }
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    let s = crate::hud::scale(camera);
    let origin = |t: &Symbol| {
        [
            (camera.width as f64 / s - t.size[0]) * 0.5,
            (camera.height as f64 / s - t.size[1]) * 0.5,
        ]
    };
    if let Some(id) = detail {
        let Some(u) = up.items.get(&id) else {
            return false;
        };
        if info {
            if let Some(unit) = unit {
                tree(v, scene, state, camera, unit, scroll);
            }
            let mut layout = c.symbols["upgrade_info"].clone();
            // UpgradeInfoDialog.updateGUIForUpgrade: calcSize all four labels,
            // then centre their combined height at82 with4/12/4pt gaps.
            let texts = [
                interface::lang(scene, &format!("#upgrade_{id}"), &u.name).to_owned(),
                interface::lang(scene, &format!("#upgrade_{id}_hint"), "").to_owned(),
                interface::lang(scene, "#required_upgrades", "").to_owned(),
                if u.parents.is_empty() {
                    interface::lang(scene, "#require_upgrades_none", "").to_owned()
                } else {
                    u.parents
                        .iter()
                        .map(|id| interface::lang(scene, &format!("#upgrade_{id}"), ""))
                        .collect::<Vec<_>>()
                        .join("\n")
                },
            ];
            let names = [
                "header_txt",
                "description_txt",
                "requiredheader_txt",
                "requiredinfo_txt",
            ];
            let heights = names.map(|name| {
                let n = layout.nodes.iter().find(|n| n.id == name).unwrap();
                crate::interface_text::candara_height(
                    n,
                    &texts[names.iter().position(|k| *k == name).unwrap()],
                )
            });
            let mut y = 82. - ((heights.iter().sum::<f64>() + 20.) as i64 / 2) as f64;
            for i in 0..4 {
                for n in &mut layout.nodes {
                    if n.id == names[i] {
                        n.rect[1] = y;
                        n.rect[3] = heights[i] + 5.;
                    }
                }
                y = (y + heights[i] + [4., 12., 4., 0.][i]).trunc();
            }
            let root = &layout;
            let o = origin(root);
            v.popup_shade(&root.nodes[0], camera);
            v.hits.clear();
            v.add(
                root,
                o,
                s,
                |_| true,
                |n| {
                    Some(match n.id.as_str() {
                        "header_txt" => {
                            interface::lang(scene, &format!("#upgrade_{id}"), &u.name).into()
                        }
                        "description_txt" => {
                            interface::lang(scene, &format!("#upgrade_{id}_hint"), "").into()
                        }
                        "requiredheader_txt" => {
                            interface::lang(scene, "#required_upgrades", "").into()
                        }
                        "requiredinfo_txt" => {
                            if u.parents.is_empty() {
                                interface::lang(scene, "#require_upgrades_none", "").into()
                            } else {
                                u.parents
                                    .iter()
                                    .map(|id| interface::lang(scene, &format!("#upgrade_{id}"), ""))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }
                        }
                        _ => String::new(),
                    })
                },
                None,
            );
            v.button(root, "close_btn", o, s, Action::Close);
            return true;
        }
        let mut layout = c.symbols["upgrade_unit"].clone();
        let count = u.items.len();
        let top = if count > 3 { count.div_ceil(2) } else { count };
        for i in 0..count {
            let first = i < top;
            let n = if first { top } else { count - top };
            let row = if first { i } else { i - top };
            let x = 325. - (((n - 1) * 2 + n * 88) / 2) as f64 + row as f64 * 90.;
            let y = if count <= 3 {
                109.
            } else if first {
                53.
            } else {
                165.
            };
            let key = format!("item{:02}", i + 1);
            let old = layout.frames[&key];
            for node in &mut layout.nodes {
                if node.parents.iter().any(|p| p == &key) {
                    node.rect[0] += x - old[0];
                    node.rect[1] += y - old[1];
                }
            }
            layout.frames.get_mut(&key).unwrap()[0] = x;
            layout.frames.get_mut(&key).unwrap()[1] = y;
        }
        let root = &layout;
        let o = origin(root);
        let inputs: Vec<_> = u.items.iter().collect();
        let quote = state.upgrade_quote(scene, u).ok();
        v.add(
            root,
            o,
            s,
            |n| {
                if n.parents
                    .iter()
                    .any(|p| p == "imagePlace" || p == "itemThumbnail")
                {
                    return false;
                }
                if let Some(i) =
                    (0..6).find(|i| n.parents.iter().any(|p| p == &format!("item{:02}", i + 1)))
                {
                    let Some((id, count)) = inputs.get(i) else {
                        return false;
                    };
                    let enough =
                        state.inventory.get(&id.to_string()).copied().unwrap_or(0) >= **count;
                    if n.id == "complete_icon" {
                        return enough;
                    }
                    if n.parents.iter().any(|p| p == "itembuy_btn") {
                        return !enough;
                    }
                }
                true
            },
            |n| {
                let i =
                    (0..6).find(|i| n.parents.iter().any(|p| p == &format!("item{:02}", i + 1)));
                Some(if let Some(i) = i {
                    inputs
                        .get(i)
                        .map_or(String::new(), |(id, count)| match n.id.as_str() {
                            "item_caption_txt" => scene.item_name(**id, "").into(),
                            "item_buy_txt" => interface::lang(scene, "#buy", "").into(),
                            "item_price_txt" => crate::pricing::special_price(scene, **id)
                                .map_or(String::new(), |n| n.to_string()),
                            "count_txt" => format!(
                                "{}/{}",
                                state
                                    .inventory
                                    .get(&id.to_string())
                                    .copied()
                                    .unwrap_or(0)
                                    .min(**count),
                                count
                            ),
                            _ => String::new(),
                        })
                } else {
                    match n.id.as_str() {
                        "caption_txt" => interface::lang(scene, "#upgrade_header", "").into(),
                        "name_txt" => {
                            interface::lang(scene, &format!("#upgrade_{id}"), &u.name).into()
                        }
                        "namehint_txt" => {
                            interface::lang(scene, &format!("#upgrade_{id}_hint"), "").into()
                        }
                        "upgradeslist_txt" => description(scene, u),
                        "upgradetime_txt" => crate::interface_resources::duration(scene, u.seconds),
                        "upgradelabel_txt" => interface::lang(scene, "#upgrade_btn", "").into(),
                        "unlockprice_txt" => {
                            quote.as_ref().map_or(String::new(), |q| q.cash.to_string())
                        }
                        "unlockname_txt" => interface::lang(scene, "#buy_for", "").into(),
                        "clarification" => interface::lang(scene, "#research_now", "").into(),
                        "startresearchhint_txt" => {
                            interface::lang(scene, "#need_to_start_research", "").into()
                        }
                        _ => String::new(),
                    }
                })
            },
            None,
        );
        v.button(root, "close_btn", o, s, Action::Close);
        if !state.upgrade_ready(id) && state.upgrade_unlocked(u) {
            v.button(root, "upgradestart_btn", o, s, Action::UpgradeStart(id));
            v.button(root, "unlock_btn", o, s, Action::UpgradeBuy(id));
        }
        for (i, (id, count)) in inputs.iter().enumerate() {
            let rect = root.frames[&format!("item{:02}", i + 1)];
            let t = &c.symbols[&format!("upgrade_unit_item{:02}", i + 1)];
            let tap = t.frames["tapZoneItemInfo"];
            v.hits.push(interface::Hit {
                action: Action::MaterialInfo(**id),
                rect: [
                    (o[0] + rect[0] + tap[0]) * s,
                    (o[1] + rect[1] + tap[1]) * s,
                    tap[2] * s,
                    tap[3] * s,
                ],
            });
            if state.inventory.get(&id.to_string()).copied().unwrap_or(0) < **count {
                v.button(
                    t,
                    "itembuy_btn",
                    [o[0] + rect[0], o[1] + rect[1]],
                    s,
                    Action::BuyMaterial(**id),
                );
            }
            if let Some(icon) = crate::interface_lab::material_icon(c, **id) {
                let center = t.frames["imagePlace"];
                // UpgradeUnitDialog.updateGUIForUpgrade:0x166a7d subtracts4
                // before converting each placeholder half extent to integer.
                let left = o[0] + rect[0] + (center[0] + center[2] * 0.5 - 4.).trunc();
                let top = o[1] + rect[1] + (center[1] + center[3] * 0.5 - 4.).trunc();
                let minx = icon
                    .nodes
                    .iter()
                    .map(|n| n.rect[0])
                    .fold(f64::INFINITY, f64::min);
                let miny = icon
                    .nodes
                    .iter()
                    .map(|n| n.rect[1])
                    .fold(f64::INFINITY, f64::min);
                let maxx = icon
                    .nodes
                    .iter()
                    .map(|n| n.rect[0] + n.rect[2])
                    .fold(f64::NEG_INFINITY, f64::max);
                let maxy = icon
                    .nodes
                    .iter()
                    .map(|n| n.rect[1] + n.rect[3])
                    .fold(f64::NEG_INFINITY, f64::max);
                v.add(
                    icon,
                    [left - (minx + maxx) * 0.5, top - (miny + maxy) * 0.5],
                    s,
                    |_| true,
                    |_| None,
                    None,
                );
            }
        }
        let thumb = root.frames["itemThumbnail"];
        icon(
            v,
            scene,
            &format!("upgrade_small_{id}"),
            [
                o[0] + thumb[0] + thumb[2] * 0.5,
                o[1] + thumb[1] + thumb[3] * 0.5,
            ],
            s,
            None,
        );
        return true;
    }
    if let Some(unit) = unit {
        return tree(v, scene, state, camera, unit, scroll);
    }
    let root = &c.symbols["upgrades"];
    let o = origin(root);
    v.add(
        root,
        o,
        s,
        |_| true,
        |n| {
            Some(if n.id == "caption_txt" {
                interface::lang(scene, "#item_1019_name", "").into()
            } else {
                String::new()
            })
        },
        None,
    );
    v.button(root, "close_btn", o, s, Action::Close);
    let clip = [(o[0] + 15.) * s, (o[1] + 65.) * s, 450. * s, 230. * s];
    let t = &c.symbols["upgrades_bcm_gui_upgradeUnit_template"];
    let width = t.size[0] + 5.;
    v.scroll_max = ((20. + width * up.unit_order.len() as f64) * s - clip[2]).max(0.);
    let offset = scroll / s;
    for (i, id) in up.unit_order.iter().enumerate() {
        let enabled = researched(scene, state, *id);
        let mut t = named(t, &format!("upgrades_unit_{id}"));
        let co = [
            o[0] + 15. + 10. + 62.5 + i as f64 * width - offset,
            o[1] + 65. + 0.5,
        ];
        let nodes: Vec<_> = up.items.values().filter(|u| u.affects(*id)).collect();
        let percent =
            nodes.iter().filter(|u| state.upgrade_ready(u.id)).count() * 100 / nodes.len().max(1);
        for n in &mut t.nodes {
            if n.parents.iter().any(|p| p == "progressbar") {
                n.rect[2] = 93. * percent as f64 / 100.;
            }
        }
        v.add(
            &t,
            co,
            s,
            |n| {
                if n.parents.iter().any(|p| p == "itemThumb") {
                    return false;
                }
                if n.parents.iter().any(|p| p == "icons_info_gray") {
                    return false;
                }
                if n.parents.iter().any(|p| p == "ability") {
                    let ability = scene
                        .combat
                        .as_ref()
                        .and_then(|c| c.defs.get(id))
                        .map_or(0, |d| d.ability);
                    let name = match ability {
                        11 => "x2damage",
                        10 => "knife",
                        7 => "armor",
                        _ => "",
                    };
                    return !name.is_empty() && n.parents.iter().any(|p| p == name);
                }
                if n.id == "imgLock" {
                    return false;
                }
                if n.parents.iter().any(|p| p == "btn_Research") {
                    return !enabled;
                }
                if n.parents.iter().any(|p| p == "btn_Upgrade") {
                    return enabled;
                }
                true
            },
            |n| {
                Some(match n.id.as_str() {
                    "itemname_txt" => scene.item_name(*id, "").into(),
                    "progress_text" => format!("{percent}%"),
                    "upgrade_txt" => interface::lang(scene, "#upgrade_btn", "").into(),
                    "research_txt" => interface::lang(scene, "#research_btn", "").into(),
                    "defence_txt" | "attack_txt" | "range_txt" | "speed_txt" => {
                        let key = match n.id.as_str() {
                            "defence_txt" => "health",
                            "range_txt" => "distance",
                            "attack_txt" => "attack",
                            _ => "speed",
                        };
                        let base = scene
                            .combat
                            .as_ref()
                            .and_then(|c| c.defs.get(id))
                            .map(|d| match key {
                                "health" => d.health,
                                "distance" => d.distance,
                                "attack" => d.attack,
                                _ => d.speed,
                            })
                            .unwrap_or(0);
                        state
                            .upgraded_stat(scene, *id, key, base as i64)
                            .to_string()
                    }
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        if let Some(texture) = state.unit_thumbnail(scene, *id) {
            picture(
                v,
                scene,
                &t,
                texture,
                t.frames["itemThumb"],
                co,
                s,
                Some(clip),
                false,
            );
        }
        let before = v.hits.len();
        v.button(
            &t,
            if enabled {
                "btn_Upgrade"
            } else {
                "btn_Research"
            },
            co,
            s,
            Action::UpgradeUnit(*id),
        );
        for h in &mut v.hits[before..] {
            let x = h.rect[0].max(clip[0]);
            let end = (h.rect[0] + h.rect[2]).min(clip[0] + clip[2]);
            h.rect[0] = x;
            h.rect[2] = (end - x).max(0.);
        }
    }
    v.hits.retain(|h| h.rect[2] > 0.);
    true
}
fn tree(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    unit: u32,
    scroll: f64,
) -> bool {
    let c = scene.interface.as_ref().unwrap();
    let up = scene.upgrades.as_ref().unwrap();
    let mut layout = c.symbols["upgrades_tree"].clone();
    let s = crate::hud::scale(camera);
    let height = camera.height as f64 / s;
    // The tree scrolls independently of its screen-filling background. The
    // retained 568x384 artwork is an authored size, not the viewport bounds.
    for node in &mut layout.nodes {
        if node.id == "background" {
            node.rect = [0., 0., camera.width as f64 / s, height];
        }
    }
    let root = &layout;
    let Ok(layers) = up.layers(unit) else {
        return false;
    };
    v.add(
        root,
        [0., 0.],
        s,
        |n| !n.parents.iter().any(|p| p == "upgrades_back"),
        |_| None,
        None,
    );
    v.scroll_max = ((layers.len() as f64 * 264. + 10.) * s - camera.width as f64).max(0.);
    let offset = scroll / s;
    let clip = [0., 0., camera.width as f64, camera.height as f64];
    let mut positions: BTreeMap<u32, [f64; 2]> = BTreeMap::new();
    for (col, layer) in layers.iter().enumerate() {
        for (row, id) in layer.iter().enumerate() {
            let u = &up.items[id];
            // Original single-node convergence columns are vertically centered,
            // including the extended model cells. The three-row bottom margin
            // does not apply to these columns (paired original ARM reference).
            let mut cy = if layer.len() == 1 {
                height * 0.5
            } else {
                height * 1.33 / (layer.len() + 1) as f64 * (row + 1) as f64 - 53.
            };
            if let Some(parent) = u.parents.first().and_then(|p| positions.get(p)) {
                if u.parents.len() == 1 || layer.len() == 2 {
                    cy = parent[1];
                }
            }
            if col > 0 && layers[col - 1].len() == 1 && layer.len() > 1 {
                cy = height / 3. * (row + 1) as f64 - 53.;
            }
            positions.insert(*id, [120. + 264. * col as f64 - offset, cy]);
        }
    }
    // Original texture segments, drawn behind cells. Position follows the DAG;
    // uncommon source path option variants still require paired visual review.
    for layer in &layers {
        for id in layer {
            let u = &up.items[id];
            let target = positions[id];
            for p in &u.parents {
                if let Some(parent) = positions.get(p) {
                    let color = if state.upgrade_ready(*p) {
                        "green"
                    } else {
                        "gray"
                    };
                    let parent_upgrade = &up.items[p];
                    let parent_size = c.symbols[if parent_upgrade.model(unit).is_some() {
                        "upgrades_tree_upgrade_cell_locked_extended"
                    } else {
                        "upgrades_tree_upgrade_cell_locked"
                    }]
                    .size;
                    let target_size = c.symbols[if u.model(unit).is_some() {
                        "upgrades_tree_upgrade_cell_locked_extended"
                    } else {
                        "upgrades_tree_upgrade_cell_locked"
                    }]
                    .size;
                    let col = layers
                        .iter()
                        .position(|layer| layer.contains(id))
                        .unwrap_or(0);
                    // Source option selects which side owns the elbow. A
                    // convergence approaches the destination vertically; a
                    // split turns beside the preceding card.
                    let option = u.parents.len() > 1
                        || col > 0 && layers[col].len() <= layers[col - 1].len();
                    draw_path(
                        v,
                        c,
                        color,
                        *parent,
                        target,
                        parent_size,
                        target_size,
                        option,
                        s,
                        clip,
                    );
                }
            }
        }
    }
    for layer in &layers {
        for id in layer {
            let u = &up.items[id];
            let ready = state.upgrade_ready(*id);
            let unlocked = state.upgrade_unlocked(u);
            let ext = u.model(unit).is_some();
            let key = format!(
                "upgrades_tree_upgrade_cell_{}{}",
                if ready {
                    "completed"
                } else if unlocked {
                    "default"
                } else {
                    "locked"
                },
                if ext { "_extended" } else { "" }
            );
            let mut t = named(&c.symbols[&key], &format!("upgrade_cell_{id}"));
            let center = positions[id];
            let o = [
                (center[0] - t.size[0] * 0.5).round(),
                (center[1] - t.size[1] * 0.5).round(),
            ];
            for n in &mut t.nodes {
                if n.id == "upgrade_icon" {
                    n.texture.clear();
                }
            }
            v.add(
                &t,
                o,
                s,
                |n| n.id != "upgrade_icon" && !n.parents.iter().any(|p| p == "upgrade_unlock_btn"),
                |n| {
                    Some(match n.id.as_str() {
                        "upgrade_name_txt" => {
                            interface::lang(scene, &format!("#upgrade_{id}"), &u.name).into()
                        }
                        "upgrade_modifires_txt" => description(scene, u)
                            .replace(": +", " +")
                            .replace(": -", " -"),
                        "upgrade_time_txt" => {
                            crate::interface_resources::duration(scene, u.seconds)
                        }
                        "upgrade_txt" => interface::lang(scene, "#upgrade_btn", "").into(),
                        "info_btn_txt" => interface::lang(scene, "#info", "").into(),
                        "unlock_btn_txt" => interface::lang(scene, "#unlock_btn", "").into(),
                        "upgrade_done_txt" => interface::lang(scene, "#status_upgraded", "").into(),
                        _ => String::new(),
                    })
                },
                Some(clip),
            );
            if let Some(icon) = c.work_icons.get(id) {
                let rect = t.frames["upgrade_icon"];
                v.add(
                    icon,
                    [
                        // UpgradesTreeDialog.updateGUI casts placeholder half
                        // extents to int before UIView2.setCenterToPoint.
                        o[0] + rect[0] + (rect[2] * 0.5).trunc(),
                        o[1] + rect[1] + (rect[3] * 0.5).trunc(),
                    ],
                    s,
                    |_| true,
                    |_| None,
                    Some(clip),
                );
            }
            if ext {
                if let Some(texture) = u
                    .model(unit)
                    .and_then(|(name, _)| scene.upgrades.as_ref()?.models.get(name))
                    .map(|m| m.thumbnail.as_str())
                    .or_else(|| c.unit_thumbs.get(&unit).map(String::as_str))
                {
                    picture(
                        v,
                        scene,
                        &t,
                        texture,
                        [176.5, 8., 89., 80.],
                        o,
                        s,
                        Some(clip),
                        false,
                    );
                }
            }
            v.button(&t, "upgrade_info_btn", o, s, Action::UpgradeInfo(*id));
            if !ready && unlocked {
                v.button(&t, "upgrade_open_btn", o, s, Action::UpgradeOpen(*id));
            }
        }
    }
    let back = root.frames["upgrades_back"];
    let o = [
        camera.width as f64 / s - 59. - back[0],
        height - 48. - back[1],
    ];
    v.add(
        root,
        o,
        s,
        |n| n.parents.iter().any(|p| p == "upgrades_back"),
        |_| None,
        None,
    );
    v.button(root, "mainmenu_btn", o, s, Action::Close);
    true
}
