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
enum ConnectorEnd {
    Side([f64; 2]),
    Vertical([f64; 2]),
    Branch([f64; 2]),
}

/// Draw an academy dependency with the client's existing dashed links and turn markers.
fn draw_path(
    v: &mut View,
    c: &interface::Config,
    completed: bool,
    [start, from_y]: [f64; 2],
    target: ConnectorEnd,
    scale: f64,
    clip: [f64; 4],
) {
    let color = if completed { "green" } else { "gray" };
    let symbol = |part: &str| c.symbols.get(&format!("upgrades_tree_{color}_{part}"));
    let add = |v: &mut View, t: &Symbol, pos: [f64; 2], tag: &str| {
        let first = v.draws.len();
        v.add(t, pos, scale, |_| true, |_| None, Some(clip));
        for draw in &mut v.draws[first..] {
            draw.node.id = tag.into();
        }
    };
    let segment = |v: &mut View, a: [f64; 2], b: [f64; 2], vertical: bool| {
        let axis = usize::from(vertical);
        let mut low = a[axis].min(b[axis]);
        let high = a[axis].max(b[axis]);
        let part = if vertical { "dash_v" } else { "dash_h" };
        let Some(t) = symbol(part) else { return };
        let dash = t.nodes[0].rect;
        let step = t.size[axis];
        if step <= 0. {
            return;
        }
        let visible_start = clip[axis] / scale;
        let visible_end = (clip[axis] + clip[axis + 2]) / scale;
        low += ((visible_start - low) / step).floor().max(0.) * step;
        while low < high.min(visible_end) {
            let length = dash[axis + 2].min(high - low);
            let mut point = a;
            point[axis] = low - dash[axis];
            let first = v.draws.len();
            add(v, t, point, "academy_connector_dash");
            for d in &mut v.draws[first..] {
                let mut clipped = clip;
                let lo = (low * scale).max(clip[axis]);
                let hi = ((low + length) * scale).min(clip[axis] + clip[axis + 2]);
                clipped[axis] = lo;
                clipped[axis + 2] = (hi - lo).max(0.);
                d.clip = Some(clipped);
            }
            low += step;
        }
    };
    let gap = 6.;
    let (direction, anchor, corner) = match target {
        ConnectorEnd::Vertical([vertical_x, to_y]) => {
            let down = to_y > from_y;
            let point = [vertical_x, to_y + if down { -gap } else { gap }];
            let corner = [vertical_x, from_y];
            let shaft = [vertical_x, point[1] + if down { -21. } else { 21. }];
            segment(v, [start, from_y], corner, false);
            segment(v, corner, shaft, true);
            (if down { "d" } else { "u" }, point, Some(corner))
        }
        ConnectorEnd::Branch([end, to_y]) => {
            let point = [end - gap, to_y];
            let corner = [start, to_y];
            segment(v, [start, from_y], corner, true);
            segment(v, corner, [point[0] - 21., to_y], false);
            ("r", point, Some(corner))
        }
        ConnectorEnd::Side([end, to_y]) => {
            let point = [end - gap, to_y];
            let shaft = [point[0] - 21., to_y];
            if (from_y - to_y).abs() < 0.01 {
                segment(v, [start, from_y], shaft, false);
                ("r", point, None)
            } else {
                let turn_radius = symbol("circle").map_or(0., |t| t.size[0] / 2.);
                let elbow =
                    ((start + shaft[0]) / 2.).max((start + turn_radius).min((start + end) / 2.));
                let corner = [elbow, from_y];
                segment(v, [start, from_y], corner, false);
                segment(v, corner, [elbow, to_y], true);
                if elbow < shaft[0] {
                    segment(v, [elbow, to_y], shaft, false);
                }
                ("r", point, Some(corner))
            }
        }
    };
    if let Some(turn) = corner
        && let Some(t) = symbol("circle")
    {
        add(v, t, turn, "academy_connector_turn");
    }
    if let Some(t) = symbol(&format!("arrow_{direction}")) {
        let [x, y, w, h] = t.nodes[0].rect;
        let offset = match direction {
            "d" => [x + w / 2., y + h],
            "u" => [x + w / 2., y],
            _ => [x + w, y + h / 2.],
        };
        add(
            v,
            t,
            [anchor[0] - offset[0], anchor[1] - offset[1]],
            "academy_connector_arrow",
        );
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
            if let Some(n) = m.values.get("incoming").and_then(|v| v.as_f64()) {
                rows.push(format!(
                    "{} {}%",
                    interface::lang(scene, "#reduce_cost", "Reduces costs"),
                    (n * 100.).abs().round() as i64
                ));
            }
            if let Some(n) = m.values.get("time").and_then(|v| v.as_i64()) {
                let seconds = n.abs();
                let (key, fallback, count) = if seconds < 60 {
                    ("#SECONDS", "[1] sec.", seconds)
                } else if seconds < 3600 {
                    ("#MINUTES", "[1] min.", seconds / 60)
                } else {
                    ("#HOURS", "[1] h", seconds / 3600)
                };
                rows.push(format!(
                    "{}: {}{}",
                    interface::lang(scene, "#production", "Production"),
                    if n < 0 { "−" } else { "+" },
                    crate::interface_resources::number_text(
                        interface::lang(scene, key, fallback),
                        count
                    )
                    .replace(' ', "\u{a0}")
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
        if let Some(unit) = unit {
            tree(v, scene, state, camera, unit, scroll);
        }
        v.hits.clear();
        v.blocks.clear();
        if info {
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
        v.popup_shade(&root.nodes[0], camera);
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
        let t = named(t, &format!("upgrades_unit_{id}"));
        let co = [
            o[0] + 15. + 10. + 62.5 + i as f64 * width - offset,
            o[1] + 65. + 0.5,
        ];
        let (completed, total) = up.items.values().filter(|u| u.affects(*id)).fold(
            (0_usize, 0_usize),
            |(completed, total), u| {
                (
                    completed + usize::from(state.upgrade_ready(u.id)),
                    total + 1,
                )
            },
        );
        let total = total.max(1);
        let fraction = completed as f64 / total as f64;
        let percent = completed * 100 / total;
        let first = v.draws.len();
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
        for draw in &mut v.draws[first..] {
            if draw.node.id == "progressbar" {
                draw.progress = fraction;
            }
        }
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
/// Optional presentation order for the fork columns that use specific rows.
/// The list is top to bottom, and zero leaves a row vacant.
fn branch_rows(unit: u32, layer: &[u32]) -> Option<[u32; 3]> {
    Some(match (unit, layer) {
        (2005, [11112, 11113]) => [11113, 11112, 0],
        (2005, [11114, 11115]) => [11115, 11114, 0],
        (2009, [11126, 11127, 11128]) => [11127, 11126, 11128],
        (2009, [11129, 11135]) => [11135, 11129, 0],
        (2011, [11055, 11183, 11184]) => [11183, 11184, 11055],
        (2011, [11185, 11186]) => [11186, 0, 11185],
        _ => return None,
    })
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
    let offset = scroll / s;
    let clip = [0., 0., camera.width as f64, camera.height as f64];
    let card_bounds = |id| {
        c.symbols[if up.items[&id].model(unit).is_some() {
            "upgrades_tree_upgrade_cell_locked_extended"
        } else {
            "upgrades_tree_upgrade_cell_locked"
        }]
        .nodes[0]
            .rect
    };
    let mut cards: BTreeMap<u32, [f64; 4]> = BTreeMap::new();
    let mut layers = layers;
    for (column, layer) in layers.iter_mut().enumerate() {
        let fixed_rows = branch_rows(unit, layer);
        // Keep connected branches in their vertical order. The sort is stable
        // so siblings with the same parent keep their catalog order.
        let parent_center = |id| {
            let mut count = 0;
            let sum = up.items[&id]
                .parents
                .iter()
                .filter_map(|parent| cards.get(parent))
                .map(|r| {
                    count += 1;
                    r[1] + r[3] / 2.
                })
                .sum::<f64>();
            if count == 0 {
                height / 2.
            } else {
                sum / count as f64
            }
        };
        layer.sort_by(|a, b| parent_center(*a).total_cmp(&parent_center(*b)));
        let rows = layer.len();
        // Two parallel branches can occupy any two of the three established
        // rows. Prefer continuing the parents' rows over recentering the pair.
        let pair = (rows == 2 && fixed_rows.is_none()).then(|| {
            let spacing = height / 3.;
            let a = parent_center(layer[0]) / spacing - 0.5;
            let b = parent_center(layer[1]) / spacing - 0.5;
            let mut best = [0., 1.];
            let score = |[top, bottom]: [f64; 2]| (top - a).abs() + (bottom - b).abs();
            for option in [[0., 2.], [1., 2.]] {
                if score(option) + 0.001 < score(best) {
                    best = option;
                }
            }
            best
        });
        for (row, id) in layer.iter().enumerate() {
            let [_, _, width, card_height] = card_bounds(*id);
            let cx = 140. + 264. * column as f64 - offset;
            let fixed_lane = fixed_rows.and_then(|rows| rows.iter().position(|v| v == id));
            let cy = if let Some(lane) = fixed_lane {
                (lane as f64 + 0.5) * height / 3.
            } else if let Some(pair) = pair {
                (pair[row] + 0.5) * height / 3.
            } else {
                height / 2. + (row as f64 - (rows as f64 - 1.) / 2.) * height / rows.max(3) as f64
            };
            cards.insert(
                *id,
                [cx - width / 2., cy - card_height / 2., width, card_height],
            );
        }
    }
    let right = cards
        .values()
        .map(|r| r[0] + r[2] + offset)
        .fold(0., f64::max);
    v.scroll_max = ((right + 14.) * s - camera.width as f64).max(0.);

    for (layer, id) in layers
        .iter()
        .flat_map(|layer| layer.iter().map(move |id| (layer, id)))
    {
        let [x, y, w, h] = cards[id];
        let center = y + h / 2.;
        let vertical_port = |from_y: f64| {
            let edge = if from_y < y - 38. {
                y
            } else if from_y > y + h + 38. {
                y + h
            } else {
                return None;
            };
            // A vertical approach may only cross empty space in this column.
            let clear = layer.iter().filter(|other| *other != id).all(|other| {
                let r = cards[other];
                r[1] >= edge.max(from_y) || r[1] + r[3] <= edge.min(from_y)
            });
            clear.then_some([x + w * 0.23, edge])
        };
        let mut parents: Vec<_> = up.items[id]
            .parents
            .iter()
            .filter_map(|parent| Some((*parent, *cards.get(parent)?)))
            .collect();
        parents.sort_by(|(_, a), (_, b)| (a[1] + a[3] / 2.).total_cmp(&(b[1] + b[3] / 2.)));
        let side = parents
            .iter()
            .map(|(_, r)| r[1] + r[3] / 2.)
            .filter(|from_y| vertical_port(*from_y).is_none())
            .collect::<Vec<_>>();
        let middle = side
            .iter()
            .position(|y| (*y - center).abs() < 0.01)
            .map_or((side.len() as f64 - 1.) / 2., |index| index as f64);
        let mut row = 0;
        for (parent, [px, py, pw, ph]) in parents {
            // Give each prerequisite its own endpoint and completion color.
            let mut from = [px + pw, py + ph / 2.];
            let to = if let Some(port) = vertical_port(from[1]) {
                ConnectorEnd::Vertical(port)
            } else {
                let to_y = center + (row as f64 - middle) * 21.;
                row += 1;
                if up.items[&parent].model(unit).is_some()
                    && (to_y < py - 24. || to_y > py + ph + 24.)
                {
                    from = [px + pw * 0.77, if to_y < py { py } else { py + ph }];
                    ConnectorEnd::Branch([x, to_y])
                } else {
                    ConnectorEnd::Side([x, to_y])
                }
            };
            draw_path(v, c, state.upgrade_ready(parent), from, to, s, clip);
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
            let [x, y, _, _] = cards[id];
            let [local_x, local_y, card_width, _] = card_bounds(*id);
            let o = [x - local_x, y - local_y];
            for n in &mut t.nodes {
                if n.id == "upgrade_icon" {
                    n.texture.clear();
                }
                if n.id == "upgrade_modifires_txt" {
                    // Extended cards have a model preview after the text.
                    // Keep the label and its wrapped lines inside the panel.
                    let right = if ext { 174. } else { local_x + card_width - 7. };
                    n.rect[2] = n.rect[2].min(right - n.rect[0]);
                    n.multiline = true;
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
