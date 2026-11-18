//! Original full-window recipe panels. Rendering and mouse actions share geometry.
use crate::{
    Camera, Scene,
    interface::{self, Action, Config, Draw, Hit, Symbol, View},
    production::{State, number},
};

fn centered(camera: &Camera, size: [f64; 2]) -> ([f64; 2], f64) {
    let s = crate::hud::scale(camera);
    (
        [
            (camera.width as f64 / s - size[0]) * 0.5,
            (camera.height as f64 / s - size[1]) * 0.5,
        ],
        s,
    )
}
fn modal(v: &mut View) {
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
}
fn research_image(
    v: &mut View,
    c: &crate::interface::Config,
    scene: &Scene,
    id: u32,
    rect: [f64; 4],
    s: f64,
    texture: Option<&str>,
) {
    let start = v.draws.len();
    image(v, c, scene, id, rect, s, None, true, texture);
    // ResearchDialog/ResearchDialogUnit explicitly use UIViewContentModeCenter4,
    // not aspect-fit1. Small sniper art must retain its natural UIImage size.
    if let Some(d) = v.draws.get_mut(start) {
        let info = &scene.textures[&d.node.texture];
        let (w, h) = (info.width as f64 / 2., info.height as f64 / 2.);
        d.node.rect = [
            (rect[0] + (rect[2] - w) / 2.) * s,
            (rect[1] + (rect[3] - h) / 2.) * s,
            w * s,
            h * s,
        ];
    }
}
fn image(
    v: &mut View,
    c: &Config,
    scene: &Scene,
    id: u32,
    rect: [f64; 4],
    s: f64,
    clip: Option<[f64; 4]>,
    unit: bool,
    texture_override: Option<&str>,
) {
    if unit {
        if let Some(t) = texture_override.or_else(|| c.unit_thumbs.get(&id).map(String::as_str)) {
            let source = &scene.textures[t];
            let ratio = (rect[2] / source.width as f64).min(rect[3] / source.height as f64);
            let w = source.width as f64 * ratio;
            let h = source.height as f64 * ratio;
            let mut n = c.symbols["training"]
                .nodes
                .iter()
                .find(|n| n.kind == "image")
                .unwrap()
                .clone();
            n.texture = t.into();
            n.id = format!("unit_{id}");
            n.parents.clear();
            n.rect = [
                (rect[0] + (rect[2] - w) * 0.5) * s,
                (rect[1] + (rect[3] - h) * 0.5) * s,
                w * s,
                h * s,
            ];
            v.draws.push(Draw {
                node: n,
                clip,
                progress: 1.,
                opacity: 255,
                grayscale: false,
            });
        }
    } else if let Some(icon) = crate::interface_lab::material_icon(c, id) {
        let bounds = icon.nodes.iter().fold(
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
        let factor = (rect[2] / (bounds[2] - bounds[0])).min(rect[3] / (bounds[3] - bounds[1]));
        let x = rect[0] + (rect[2] - (bounds[2] - bounds[0]) * factor) * 0.5 - bounds[0] * factor;
        let y = rect[1] + (rect[3] - (bounds[3] - bounds[1]) * factor) * 0.5 - bounds[1] * factor;
        v.add(
            icon,
            [x / factor, y / factor],
            s * factor,
            |_| true,
            |_| None,
            clip,
        );
    }
}
fn time(seconds: i64) -> String {
    let n = seconds.max(0);
    if n >= 3600 {
        format!("{}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60)
    } else {
        format!("{}:{:02}", n / 60, n % 60)
    }
}
fn button_clip(
    v: &mut View,
    symbol: &Symbol,
    id: &str,
    o: [f64; 2],
    s: f64,
    clip: [f64; 4],
    a: Action,
) {
    let len = v.hits.len();
    v.button(symbol, id, o, s, a);
    if v.hits.len() > len {
        let r = &mut v.hits[len].rect;
        let x = r[0].max(clip[0]);
        let y = r[1].max(clip[1]);
        let w = (r[0] + r[2]).min(clip[0] + clip[2]) - x;
        let h = (r[1] + r[3]).min(clip[1] + clip[3]) - y;
        if w <= 0. || h <= 0. {
            v.hits.pop();
        } else {
            *r = [x, y, w, h];
        }
    }
}
/// One original horizontal list; index is an actual recipe index, not a save edit.
pub fn workshop(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    uid: &str,
    scroll: f64,
    now: i64,
) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(e) = state.entities.get(uid) else {
        return false;
    };
    let Some(work) = &scene.workshop else {
        return false;
    };
    let Some(ids) = work.buildings.get(&(number(e, "type") as u32)) else {
        return false;
    };
    let key = if number(e, "type") == 1038 {
        "laboratory"
    } else {
        "training"
    };
    let Some(root) = c.symbols.get(key) else {
        return false;
    };
    modal(v);
    let (o, s) = centered(camera, root.size);
    let (used, max) = state.population(scene);
    v.add(
        root,
        o,
        s,
        |n| !n.id.starts_with("scroll_side"),
        |n| {
            Some(match n.id.as_str() {
                "header_txt" => scene
                    .item_name(
                        number(e, "type") as u32,
                        interface::lang(scene, "#training", "Обучение"),
                    )
                    .into(),
                "caption_txt" => interface::lang(scene, "#laboratory", "Лаборатория").into(),
                "hud_play_money_txt" => state.resources["playMoney"].to_string(),
                "hud_real_money_txt" => state.resources["realMoney"].to_string(),
                "hud_oil_txt" => state.resources["fuel"].to_string(),
                "hud_iron_txt" => state.resources["iron"].to_string(),
                "hud_token_txt" | "hud_tokens_txt" | "hud_capacity_txt" => format!("{used}/{max}"),
                _ => String::new(),
            })
        },
        None,
    );
    let laboratory = key == "laboratory";
    let stride = if laboratory { 130. } else { 127.5 };
    let width = if laboratory {
        let l = root.frames["scroll_side_left"];
        root.frames["scroll_side_right"][0] + l[2] - l[0]
    } else {
        446.
    };
    let clip = [(o[0] + 17.) * s, (o[1] + 70.) * s, width * s, 236. * s];
    v.scroll_max =
        (ids.len() as f64 * stride + if laboratory { 2. } else { 0. } - width).max(0.) * s;
    let shift = scroll / s;
    for (index, id) in ids.iter().enumerate() {
        let sequence = if number(e, "productionType") == *id as i64 {
            state.workshop_origin_sequence(uid, *id, number(e, "productionStartTime"), u64::MAX)
        } else {
            state.revision + 1
        };
        let r = state.upgraded_recipe(scene, &work.recipes[id], sequence);
        let locked = state.workshop_access(&r).is_err();
        let template =
            &c.symbols[&format!("{key}_item_{}", if locked { "locked" } else { "normal" })];
        let prepared = laboratory.then(|| crate::interface_lab::card(template, &r));
        let template = prepared.as_ref().unwrap_or(template);
        let origin = [
            o[0] + if laboratory { 81.5 } else { 79.5 } + index as f64 * stride - shift,
            o[1] + if laboratory { 75. } else { 70. },
        ];
        let active = number(e, "productionType") == *id as i64;
        let busy = number(e, "productionType") != 0;
        let remain = if active {
            (number(e, "productionStartTime") + r.seconds - now).max(0)
        } else {
            r.seconds
        };
        let inputs: Vec<_> = r.items.iter().collect();
        let ability = scene
            .combat
            .as_ref()
            .and_then(|b| b.defs.get(&r.output))
            .map_or("", |d| match d.ability {
                11 => "x2damage",
                10 => "knife",
                7 => "armor",
                _ => "",
            });
        let begin = v.draws.len();
        v.add(
            template,
            origin,
            s,
            |n| {
                for (prefix, key) in [
                    ("play_money_", "playMoney"),
                    ("oil_", "fuel"),
                    ("iron_", "iron"),
                ] {
                    if n.id.starts_with(prefix) {
                        return r.cost.get(key).is_some_and(|v| *v > 0);
                    }
                }
                !(n.parents.iter().any(|p| p == "ability")
                    && (ability.is_empty() || n.id != ability))
                    && !n
                        .parents
                        .iter()
                        .any(|p| ["item_image", "item1_icon", "item2_icon"].contains(&p.as_str()))
                    && !["lock_limit_txt", "unlock_btn"].contains(&n.id.as_str())
            },
            |n| {
                Some(match n.id.as_str() {
                    "caption_txt" => if laboratory {
                        interface::lang(scene, &format!("#recipe_{}", r.id), &r.name)
                    } else {
                        scene.item_name(r.output, &r.name)
                    }
                    .into(),
                    "time_txt" => {
                        if laboratory && !active {
                            crate::interface_lab::recipe_time(scene, remain)
                        } else if !laboratory && !active {
                            crate::interface_resources::duration(scene, remain)
                        } else {
                            time(remain)
                        }
                    }
                    "button_txt" | "Create_txt" => if active {
                        if remain == 0 {
                            "Забрать"
                        } else {
                            "Готовится"
                        }
                    } else {
                        if locked {
                            if r.unit() && r.research != 0 && !state.research_ready(r.research) {
                                interface::lang(scene, "#research_btn", "Исследовать")
                            } else {
                                "Закрыто"
                            }
                        } else if r.unit() {
                            "Тренировать"
                        } else {
                            "Создать"
                        }
                    }
                    .into(),
                    "token_txt" => work
                        .population
                        .get(&r.output)
                        .copied()
                        .unwrap_or(0)
                        .to_string(),
                    "unlock_txt" if laboratory => interface::lang(scene, "#unlock_btn", "").into(),
                    "unlock_price_txt" if laboratory => r.unlock_price.to_string(),
                    "lock_txt" => {
                        if r.research != 0 && !state.research_ready(r.research) {
                            scene
                                .research
                                .as_ref()
                                .and_then(|c| c.items.get(&r.research))
                                .map(|research| {
                                    interface::lang(
                                        scene,
                                        &format!("#research_{}", research.id),
                                        &research.name,
                                    )
                                    .to_owned()
                                })
                                .unwrap_or_else(|| "Нужно исследование".into())
                        } else {
                            format!("{} уровень", r.min_level)
                        }
                    }
                    "play_money_txt" => r.cost.get("playMoney").copied().unwrap_or(0).to_string(),
                    "oil_txt" => r.cost.get("fuel").copied().unwrap_or(0).to_string(),
                    "iron_txt" => r.cost.get("iron").copied().unwrap_or(0).to_string(),
                    "item1_txt" => inputs.first().map_or(String::new(), |(_, n)| n.to_string()),
                    "item2_txt" => inputs.get(1).map_or(String::new(), |(_, n)| n.to_string()),
                    "defence_txt" | "attack_txt" | "range_txt" | "speed_txt" => scene
                        .combat
                        .as_ref()
                        .and_then(|b| b.defs.get(&r.output))
                        .map_or(String::new(), |d| {
                            let (key, base) = match n.id.as_str() {
                                "defence_txt" => ("health", d.health),
                                "attack_txt" => ("attack", d.attack),
                                "range_txt" => ("distance", d.distance),
                                _ => ("speed", d.speed),
                            };
                            state
                                .upgraded_stat(scene, r.output, key, base as i64)
                                .to_string()
                        }),
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        if !laboratory {
            let costs: Vec<_> = [
                ("play_money", "playMoney"),
                ("oil", "fuel"),
                ("iron", "iron"),
            ]
            .into_iter()
            .filter(|(_, k)| r.cost.get(*k).is_some_and(|v| *v > 0))
            .collect();
            for (i, (prefix, _)) in costs.iter().enumerate() {
                let target = origin[0]
                    + 1.
                    + (i as f64 - (costs.len().saturating_sub(1)) as f64 * 0.5) * 30.;
                for d in &mut v.draws[begin..] {
                    if d.node.id == format!("{prefix}_icon") || d.node.id == format!("{prefix}_txt")
                    {
                        d.node.rect[0] = target * s - d.node.rect[2] * 0.5;
                    }
                }
            }
            let bottom = template.buttons.get("buy_btn").map_or(196.5, |r| r[1]);
            for d in &mut v.draws[begin..] {
                if d.node.id == "lock_txt" {
                    let height = crate::interface_text::candara_height(&d.node, &d.node.text)
                        .min(d.node.rect[3]);
                    d.node.rect[1] = d.node.rect[1].min((origin[1] + bottom - 3.) * s - height);
                }
            }
        }
        if busy && !active {
            for d in &mut v.draws[begin..] {
                if d.node
                    .parents
                    .iter()
                    .any(|p| p == "buy_btn" || p == "btn_create")
                {
                    d.opacity = 128;
                }
            }
        }
        let slot = if r.unit() { "unit_image" } else { "item_image" };
        if let Some(rect) = template
            .frames
            .get(slot)
            .or_else(|| template.slots.get(slot))
        {
            let rect = [origin[0] + rect[0], origin[1] + rect[1], rect[2], rect[3]];
            let picture_start = v.draws.len();
            if !laboratory
                || !crate::interface_lab::thumbnail(v, c, scene, r.output, rect, s, clip, locked)
            {
                image(
                    v,
                    c,
                    scene,
                    r.output,
                    rect,
                    s,
                    Some(clip),
                    r.unit(),
                    state.unit_thumbnail(scene, r.output),
                );
            }
            if r.unit() {
                // TrainingDialog UIImageView.contentMode=Center (4), not
                // AspectFit: the phone thumbnail keeps its natural size.
                if let Some(t) = state.unit_thumbnail(scene, r.output) {
                    let meta = &scene.textures[t];
                    for d in &mut v.draws[picture_start..] {
                        let w = meta.width as f64 * 0.5 * s;
                        let h = meta.height as f64 * 0.5 * s;
                        d.node.rect = [
                            (rect[0] + rect[2] * 0.5) * s - w * 0.5,
                            (rect[1] + rect[3] * 0.5) * s - h * 0.5,
                            w,
                            h,
                        ];
                        d.grayscale = locked;
                    }
                }
            }
            if laboratory {
                if let Some(lock) = v.draws[begin..picture_start]
                    .iter()
                    .position(|d| d.node.id == "lock_icon")
                {
                    let pictures: Vec<_> = v.draws.drain(picture_start..).collect();
                    v.draws.splice(begin + lock..begin + lock, pictures);
                }
            }
        }
        for (i, (id, _)) in inputs.iter().take(2).enumerate() {
            if let Some(rect) = template
                .frames
                .get(&format!("item{}_icon", i + 1))
                .or_else(|| template.slots.get(&format!("item{}_icon", i + 1)))
            {
                let rect = [origin[0] + rect[0], origin[1] + rect[1], rect[2], rect[3]];
                if laboratory {
                    crate::interface_lab::material(v, c, **id, rect, s, clip);
                } else {
                    image(v, c, scene, **id, rect, s, Some(clip), false, None);
                }
            }
        }
        if laboratory {
            for d in &mut v.draws[begin..] {
                d.node
                    .parents
                    .insert(0, format!("source_lab_recipe_{}", r.id));
            }
        }
        if laboratory && locked && r.unlock_price > 0 {
            button_clip(
                v,
                template,
                "btn_unlock",
                origin,
                s,
                clip,
                Action::RecipeAccess(*id),
            );
        }
        if !locked && (!busy || active) {
            button_clip(
                v,
                template,
                if r.unit() { "buy_btn" } else { "btn_create" },
                origin,
                s,
                clip,
                Action::Produce(index),
            );
        } else if locked && r.unit() && r.research != 0 && !state.research_ready(r.research) {
            button_clip(
                v,
                template,
                "buy_btn",
                origin,
                s,
                clip,
                Action::UpgradeUnit(r.output),
            );
        }
    }
    v.button(root, "close_btn", o, s, Action::Close);
    true
}
/// Notices and confirmations use the original MessageBox symbol, never a footer.
pub fn message(v: &mut View, scene: &Scene, camera: &Camera, text: &str, confirm: bool) {
    let Some(c) = &scene.interface else {
        return;
    };
    let Some(root) = c.symbols.get("message") else {
        return;
    };
    let mut popup = root.clone();
    // MessageBoxDialog.initWithCaption:btnOK:btnCancel: centers the remaining
    // OK button in its superview when cancel is empty. Move its hit target too.
    if !confirm {
        let button = popup.buttons.get_mut("ok_btn").unwrap();
        let dx = popup.size[0] * 0.5 - (button[0] + button[2] * 0.5);
        button[0] += dx;
        for n in &mut popup.nodes {
            if n.parents.iter().any(|p| p == "ok_btn") {
                n.rect[0] += dx;
            }
        }
    }
    for n in &mut popup.nodes {
        if n.id == "caption_txt" {
            n.parents.push("source_message_caption".into());
        }
    }
    let root = &popup;
    v.modal = true;
    v.hits.clear();
    let (o, s) = centered(camera, root.size);
    v.popup_shade(&root.nodes[0], camera);
    v.add(
        root,
        o,
        s,
        |n| confirm || !n.parents.iter().any(|p| p == "cancel_btn"),
        |n| {
            Some(match n.id.as_str() {
                "caption_txt" => text.into(),
                _ => {
                    if n.parents.iter().any(|p| p == "cancel_btn") {
                        interface::lang(scene, "#NO", "Нет").into()
                    } else if n.parents.iter().any(|p| p == "ok_btn") {
                        if confirm {
                            interface::lang(scene, "#YES", "Да").into()
                        } else {
                            "OK".into()
                        }
                    } else {
                        String::new()
                    }
                }
            })
        },
        None,
    );
    v.button(
        root,
        "ok_btn",
        o,
        s,
        if confirm {
            Action::Confirm
        } else {
            Action::Dismiss
        },
    );
    v.button(root, "close_btn", o, s, Action::Dismiss);
    if confirm {
        v.button(root, "cancel_btn", o, s, Action::Dismiss);
    }
}

/// Source ResearchDialog::updateGUI uses this explicit six-card order.
pub const RESEARCH_ORDER: [u32; 6] = [10000, 10002, 10004, 10003, 10005, 10006];
pub fn research(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    uid: &str,
    choice: Option<usize>,
    now: i64,
) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(rc) = &scene.research else {
        return false;
    };
    let Some(e) = state
        .entities
        .get(uid)
        .filter(|e| number(e, "type") == 1020)
    else {
        return false;
    };
    let key = if choice.is_some() {
        "research_unit"
    } else {
        "research"
    };
    let Some(root) = c.symbols.get(key) else {
        return false;
    };
    modal(v);
    let (o, s) = centered(camera, root.size);
    if let Some(index) = choice {
        let Some(r) = RESEARCH_ORDER.get(index).and_then(|id| rc.items.get(id)) else {
            return false;
        };
        let inputs: Vec<_> = r.items.iter().collect();
        let active = number(e, "productionType") == r.id as i64;
        let ready = state.research_ready(r.id);
        let paid = scene.pricing.is_some()
            && !ready
            && number(e, "productionType") == 0
            && state.research_dialog_access(scene, r).is_ok();
        let quote = if paid {
            state.research_quote(scene, r).ok()
        } else {
            None
        };

        let remain = if active {
            (number(e, "productionStartTime") + r.seconds - now).max(0)
        } else {
            r.seconds
        };
        let mut layout = root.clone();
        // ResearchDialogUnit.updateGUI centres the visible ingredient cards
        // at325pt, using their88pt frames and5pt spacing (one to three).
        let input_x =
            325. - (inputs.len() as f64 * 88. + inputs.len().saturating_sub(1) as f64 * 5.) / 2.;
        for i in 0..inputs.len() {
            let group = format!("item{:02}", i + 1);
            let old = root
                .frames
                .get(&group)
                .map_or(189.5 + i as f64 * 90.5, |r| r[0]);
            let dx = input_x + i as f64 * 93. - old;
            for n in &mut layout.nodes {
                if n.parents.contains(&group) {
                    n.rect[0] += dx;
                }
            }
            for (name, rect) in &mut layout.buttons {
                if name.starts_with(&group) {
                    rect[0] += dx;
                }
            }
            if let Some(slot) = layout.slots.get_mut(&group) {
                slot[0] += dx;
            }
            if let Some(frame) = layout.frames.get_mut(&group) {
                frame[0] += dx;
            }
        }
        let root = &layout;
        let cost_resource = |n: &interface::Node| {
            [
                ("play_money", "playMoney"),
                ("oil", "fuel"),
                ("iron", "iron"),
            ]
            .into_iter()
            .find(|(group, _)| n.parents.iter().any(|p| p == group))
            .map(|(_, key)| key)
        };
        let start = v.draws.len();
        v.add(
            root,
            o,
            s,
            |n| {
                if n.parents
                    .iter()
                    .any(|p| p == "tapZoneItemInfo" || p == "imagePlace")
                {
                    return false;
                }
                if cost_resource(n).is_some_and(|key| r.cost.get(key).copied().unwrap_or(0) == 0) {
                    return false;
                }
                if (n.parents.iter().any(|p| p == "researchbuy_btn")
                    || n.id == "research_hint"
                    || (n.kind == "image" && n.rect == [22.5, 237.5, 160., 68.]))
                    && quote.is_none()
                {
                    return false;
                }
                if let Some(i) =
                    (0..3).find(|i| n.parents.iter().any(|p| p == &format!("item{:02}", i + 1)))
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
                        return !enough
                            && scene.pricing.is_some()
                            && !ready
                            && crate::pricing::special_price(scene, **id).is_ok();
                    }
                }
                true
            },
            |n| {
                let item_index =
                    (0..3).find(|i| n.parents.iter().any(|p| p == &format!("item{:02}", i + 1)));
                Some(if let Some(i) = item_index {
                    inputs
                        .get(i)
                        .map_or(String::new(), |(id, count)| match n.id.as_str() {
                            "item_caption_txt" => scene.item_name(**id, "").into(),
                            "item_buy_txt" => interface::lang(scene, "#BUY", "Купить").into(),
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
                        "caption_txt" => {
                            interface::lang(scene, "#research_header", "Исследования").into()
                        }
                        "name_txt" => {
                            interface::lang(scene, &format!("#research_{}", r.id), &r.name).into()
                        }
                        "research_hint" => {
                            interface::lang(scene, "#research_now", "Мгновенное исследование")
                                .into()
                        }
                        "unlockbuy_txt" => interface::lang(scene, "#buy_for", "Купить за").into(),
                        "unlockprice_txt" => {
                            quote.as_ref().map_or(String::new(), |q| q.cash.to_string())
                        }
                        "startresearch_hint" => {
                            interface::lang(scene, "#need_to_start_research", "").into()
                        }
                        "researchtime_txt" => crate::interface_resources::duration(scene, remain),
                        "researchlabel_txt" => if ready {
                            "Исследовано"
                        } else if active {
                            if remain == 0 {
                                "Завершить"
                            } else {
                                "Исследуется"
                            }
                        } else {
                            "Исследовать"
                        }
                        .into(),
                        "button_txt" | "research1btn_txt" => if ready {
                            "Исследовано"
                        } else if active {
                            if remain == 0 {
                                "Завершить"
                            } else {
                                "Исследуется"
                            }
                        } else {
                            "Исследовать"
                        }
                        .into(),
                        "defence_txt" | "attack_txt" | "range_txt" | "speed_txt" => scene
                            .combat
                            .as_ref()
                            .and_then(|b| b.defs.get(&r.unit))
                            .map_or(String::new(), |d| {
                                let (key, base) = match n.id.as_str() {
                                    "defence_txt" => ("health", d.health),
                                    "attack_txt" => ("attack", d.attack),
                                    "range_txt" => ("distance", d.distance),
                                    _ => ("speed", d.speed),
                                };
                                state
                                    .upgraded_stat(scene, r.unit, key, base as i64)
                                    .to_string()
                            }),
                        _ => {
                            if n.parents.iter().any(|p| p == "play_money") {
                                r.cost.get("playMoney").copied().unwrap_or(0).to_string()
                            } else if n.parents.iter().any(|p| p == "oil") {
                                r.cost.get("fuel").copied().unwrap_or(0).to_string()
                            } else if n.parents.iter().any(|p| p == "iron") {
                                r.cost.get("iron").copied().unwrap_or(0).to_string()
                            } else {
                                String::new()
                            }
                        }
                    }
                })
            },
            None,
        );
        // ResearchDialogUnit.updateGUI colors each missing currency red;
        // ingredient cards retain their separate owned/required counters.
        for d in &mut v.draws[start..] {
            if d.node.kind == "text"
                && cost_resource(&d.node).is_some_and(|key| {
                    state.resources.get(key).copied().unwrap_or(0)
                        < r.cost.get(key).copied().unwrap_or(0)
                })
            {
                d.node.color = 0xff0000;
            }
        }
        research_image(
            v,
            c,
            scene,
            r.unit,
            [o[0] + 40., o[1] + 38.5, 120., 114.5],
            s,
            state.unit_thumbnail(scene, r.unit),
        );
        for (i, (id, _)) in inputs.iter().enumerate() {
            image(
                v,
                c,
                scene,
                **id,
                [
                    o[0] + input_x + i as f64 * 93. + 26.5,
                    o[1] + 119. + 47.,
                    35.,
                    41.,
                ],
                s,
                None,
                false,
                None,
            );
        }
        if quote.is_some() {
            v.button(root, "researchbuy_btn", o, s, Action::BuyResearch(r.id));
        }
        for (i, (id, count)) in inputs.iter().enumerate() {
            let group = format!("item{:02}", i + 1);
            if let (Some(card), Some(rect)) = (
                c.symbols.get(&format!("research_unit_{group}")),
                root.frames.get(&group),
            ) {
                if let Some(tap) = card.frames.get("tapZoneItemInfo") {
                    v.hits.push(interface::Hit {
                        action: Action::MaterialInfo(**id),
                        rect: [
                            (o[0] + rect[0] + tap[0]) * s,
                            (o[1] + rect[1] + tap[1]) * s,
                            tap[2] * s,
                            tap[3] * s,
                        ],
                    });
                }
            }
            if !ready
                && scene.pricing.is_some()
                && state.inventory.get(&id.to_string()).copied().unwrap_or(0) < **count
                && crate::pricing::special_price(scene, **id).is_ok()
            {
                let key = format!("item{:02}", i + 1);
                if let (Some(card), Some(slot)) = (
                    c.symbols.get(&format!("research_unit_{key}")),
                    root.slots.get(&key),
                ) {
                    v.button(
                        card,
                        "itembuy_btn",
                        [o[0] + slot[0], o[1] + slot[1]],
                        s,
                        Action::BuyMaterial(**id),
                    );
                }
            }
        }
        if !ready {
            v.button(
                root,
                "researchstart_btn",
                o,
                s,
                Action::Produce(rc.items.keys().position(|id| *id == r.id).unwrap()),
            );
        }
    } else {
        v.add(
            root,
            o,
            s,
            |n| !n.parents.iter().any(|p| p.starts_with("item0")),
            |n| {
                Some(if n.id == "caption_txt" {
                    scene.item_name(1020, "Центр исследований").into()
                } else {
                    String::new()
                })
            },
            None,
        );
        for (i, id) in RESEARCH_ORDER.iter().enumerate() {
            let Some(r) = rc.items.get(id) else {
                continue;
            };
            let name = format!("item{i:02}");
            let template = &c.symbols[&format!("research_{name}")];
            let slot = &root.slots[&name];
            let origin = [o[0] + slot[0], o[1] + slot[1]];
            let ready = state.research_ready(*id);
            let locked = state.research_dialog_access(scene, r).is_err() && !ready;
            let start = v.draws.len();
            v.add(
                template,
                origin,
                s,
                |n| {
                    !n.parents.iter().any(|p| p == "place_image1")
                        && (n.id != "imgBottom1" || !locked)
                        && (n.id != "imgBottom1Gray" || locked)
                        && (!n.parents.iter().any(|p| p == "access1_btn")
                            || (state.can_buy_research_access(r) && scene.pricing.is_some()))
                        && (!n.parents.iter().any(|p| p == "research1_btn") || (!ready && !locked))
                        && (n.id != "locked1" || locked)
                        && (n.id != "done1" || ready)
                        && (n.id != "level1_txt" || locked)
                },
                |n| {
                    Some(match n.id.as_str() {
                        "research1_txt" => {
                            interface::lang(scene, &format!("#research_{id}"), &r.name).into()
                        }
                        "unlock1_txt" => r.unlock_price.to_string(),
                        "accesslabel1_txt" => {
                            interface::lang(scene, "#unlock_btn", "Доступ").into()
                        }
                        "level1_txt" => {
                            if r.min_level > state.resources["level"] {
                                format!("{} уровень", r.min_level)
                            } else if !state.research_ready(r.parent) {
                                rc.items.get(&r.parent).map_or(String::new(), |r| {
                                    scene.item_name(r.unit, &r.name).into()
                                })
                            } else {
                                scene.item_name(r.entity, "").into()
                            }
                        }
                        "button_txt" | "research1btn_txt" => if ready {
                            "Исследовано"
                        } else {
                            "Исследовать"
                        }
                        .into(),
                        _ => String::new(),
                    })
                },
                None,
            );
            research_image(
                v,
                c,
                scene,
                r.unit,
                [origin[0] - 57., origin[1] - 4., 115., 105.],
                s,
                state.unit_thumbnail(scene, r.unit),
            );
            // Source replaces place_image1 IN the retained hierarchy, after
            // imgBottom1 and before requirement labels/locks/buttons. Appending
            // the thumbnail to the whole view paints over those controls.
            let end = v.draws.len();
            if end > start && v.draws[end - 1].node.id == format!("unit_{}", r.unit) {
                let mut thumb = v.draws.pop().unwrap();
                thumb.grayscale = locked;
                let at = v.draws[start..]
                    .iter()
                    .position(|d| d.node.id == "imgBottom1")
                    .map_or(start, |i| start + i + 1);
                v.draws.insert(at, thumb);
                for d in &mut v.draws[start..] {
                    if d.node.id == "imgBottom1" {
                        d.grayscale = locked;
                    }
                }
            }
            if !ready && !locked {
                v.button(template, "research1_btn", origin, s, Action::Choose(i));
            }
            if state.can_buy_research_access(r) && scene.pricing.is_some() {
                v.button(
                    template,
                    "access1_btn",
                    origin,
                    s,
                    Action::ResearchAccess(*id),
                );
            }
        }
    }
    v.button(root, "close_btn", o, s, Action::Close);
    true
}

/// Original left-hand quest buttons; draw before inventory shading/settings paper.
pub fn quest_badges(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    interactive: bool,
) {
    let Some(c) = &scene.interface else {
        return;
    };
    let s = crate::hud::scale(camera);
    for (i, q) in state.active_quests(scene).iter().enumerate() {
        let Some(symbol) = c
            .quest_icons
            .get(&q.id)
            .and_then(|name| c.quest_symbols.get(name))
        else {
            continue;
        };
        let o = [33., 85. + 52. * i as f64];
        v.add(symbol, o, s, |_| true, |_| None, None);
        if interactive {
            let rect = [
                (o[0] - symbol.size[0] * 0.5) * s,
                (o[1] - symbol.size[1] * 0.5) * s,
                symbol.size[0] * s,
                symbol.size[1] * s,
            ];
            v.hits.push(crate::interface::Hit {
                action: Action::Quest(i),
                rect,
            });
            v.blocks.push(rect);
        }
    }
}
pub fn quest_notifications(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    seconds: f64,
) {
    let Some(c) = &scene.interface else { return };
    let Some(root) = c.symbols.get("quest_notification") else {
        return;
    };
    // SceneMain.updateQuestNotificationsForID:65 ->90, one-second easeOut
    // reversing tween. updateTimers hides after5s; its long timer admits again.
    let s = crate::hud::scale(camera);
    let cycle = seconds.max(0.) % 15.;
    if cycle >= 6. {
        return;
    }
    let p = cycle % 2.;
    let p = if p < 1. { p } else { 2. - p };
    let x = 65. + 25. * (1. - (1. - p).powi(2));
    for (i, q) in state.active_quests(scene).iter().enumerate() {
        if !state.quest_story_new(scene, q.id) {
            continue;
        }
        v.add(
            root,
            [x, 85. + 52. * i as f64],
            s,
            |_| true,
            |n| {
                (n.id == "caption_txt")
                    .then(|| interface::lang(scene, "#new_quest", "Новое задание!").into())
            },
            None,
        );
        let r = [
            (x - 2.5) * s,
            (85. + 52. * i as f64 - 11.5) * s,
            140. * s,
            33. * s,
        ];
        v.hits.push(Hit {
            action: Action::Quest(i),
            rect: r,
        });
        v.blocks.push(r);
    }
}

fn award_group(group: &str) -> Option<&'static str> {
    match group {
        "play_money" => Some("playMoney"),
        "real_money" => Some("realMoney"),
        "exp" => Some("experience"),
        "oil" => Some("fuel"),
        "iron" => Some("iron"),
        "item" => Some("item"),
        "gold" => Some("gold"),
        "uran" => Some("uran"),
        _ => None,
    }
}
fn rewards(
    v: &mut View,
    root: &Symbol,
    q: &crate::systems::Quest,
    scene: &Scene,
    o: [f64; 2],
    s: f64,
    clip: Option<[f64; 4]>,
) {
    // QuestCompleteDialog::updateGUI and QuestDialog::updateGUI both place
    // only visible reward views at 50-point intervals, centred as a group.
    let groups: Vec<_> = ["play_money", "exp", "oil", "iron", "item", "real_money"]
        .into_iter()
        .filter(|g| {
            q.awards
                .iter()
                .any(|a| Some(a.kind.as_str()) == award_group(g))
        })
        .collect();
    let mut offsets = std::collections::BTreeMap::new();
    for (i, g) in groups.iter().enumerate() {
        if let Some(r) = root.frames.get(*g) {
            offsets.insert(
                *g,
                root.size[0] * 0.5 - (groups.len().saturating_sub(1) as f64) * 25. + i as f64 * 50.
                    - (r[0] + r[2] * 0.5),
            );
        }
    }
    let begin = v.draws.len();
    v.add(
        root,
        o,
        s,
        |n| {
            n.parents
                .iter()
                .filter_map(|p| award_group(p))
                .all(|k| q.awards.iter().any(|a| a.kind == k))
        },
        |n| {
            let award = n
                .parents
                .iter()
                .find_map(|p| award_group(p))
                .and_then(|k| q.awards.iter().find(|a| a.kind == k));
            Some(if let Some(a) = award {
                if n.kind == "text" {
                    format!("+{}", a.count)
                } else {
                    String::new()
                }
            } else if n.id == "rewards_caption_txt" {
                interface::lang(scene, "#YOUR_REWARD_IS", "Твоя награда:").into()
            } else {
                String::new()
            })
        },
        clip,
    );
    for d in &mut v.draws[begin..] {
        if let Some(dx) = d.node.parents.iter().find_map(|p| offsets.get(p.as_str())) {
            d.node.rect[0] += dx * s;
        }
    }
    if let Some(a) = q.awards.iter().find(|a| a.kind == "item") {
        if let (Some(c), Some(rect)) = (&scene.interface, root.frames.get("item_icon")) {
            image(
                v,
                c,
                scene,
                a.item_id,
                [
                    o[0] + rect[0] + offsets.get("item").copied().unwrap_or(0.),
                    o[1] + rect[1],
                    rect[2],
                    rect[3],
                ],
                s,
                clip,
                false,
                None,
            );
        }
    }
}
fn exercise_copy(scene: &Scene, e: &crate::systems::Exercise, text: &str) -> String {
    let mut text = text.to_owned();
    if let Some(u) = scene
        .upgrades
        .as_ref()
        .and_then(|c| c.items.get(&e.item_id))
    {
        text = text.replace(
            "[upgrade_name]",
            interface::lang(scene, &format!("#upgrade_{}", u.id), &u.name),
        );
        if let Some(unit) = scene
            .upgrades
            .as_ref()
            .and_then(|c| c.unit_order.iter().find(|id| u.affects(**id)))
        {
            text = text.replace("[unit_name]", scene.item_name(*unit, ""));
        }
    }
    // Missions::getLocalizedName reads #item_<id>_name, not the map's alias.
    let name = scene.item_name(e.item_id, "");
    if !name.is_empty() {
        return text.replace("[mission_name]", name);
    }
    if let Some(m) = scene
        .combat
        .as_ref()
        .and_then(|c| c.missions.values().find(|m| m.mission_id == e.item_id))
    {
        text.replace("[mission_name]", &m.title)
    } else {
        text
    }
}
/// Original quest page and completion card. Counters and claims use the native
/// original quest model; scrolling changes only presentation, never progress.
pub fn quests(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    index: usize,
    scroll: f64,
) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(root) = c.symbols.get("quests") else {
        return false;
    };
    let active = state.active_quests(scene);
    let Some(q) = active.get(index % active.len().max(1)) else {
        return false;
    };
    modal(v);
    if state.quest_ready(scene, q) {
        let root = &c.symbols["quest_complete"];
        if let Some(n) = root.nodes.first() {
            v.popup_shade(n, camera);
        }
        let (o, s) = centered(camera, root.size);
        let start = v.draws.len();
        v.add(
            root,
            o,
            s,
            |n| !n.parents.iter().any(|p| award_group(p).is_some()),
            |n| {
                Some(match n.id.as_str() {
                    "name_txt" => q.name.clone(),
                    "header_txt" => {
                        interface::lang(scene, "#QUEST_COMPLETED", "Задание выполнено!").into()
                    }
                    "award_txt" => {
                        interface::lang(scene, "#YOUR_REWARD_IS", "Твоя награда:").into()
                    }
                    _ => {
                        if n.parents.iter().any(|p| p == "ok_btn") {
                            interface::lang(scene, "#ok", "OK").into()
                        } else {
                            String::new()
                        }
                    }
                })
            },
            None,
        );
        if let Some(template) = root.nodes.iter().find(|n| n.id == "name_txt") {
            let offset = if crate::interface_text::title_lines(template, &q.name) == 1 {
                15.
            } else {
                7.
            };
            for d in &mut v.draws[start..] {
                if d.node.id == "name_txt" {
                    d.node.rect[1] += offset * s;
                }
            }
        }
        let only = Symbol {
            size: root.size,
            nodes: root
                .nodes
                .iter()
                .filter(|n| n.parents.iter().any(|p| award_group(p).is_some()))
                .cloned()
                .collect(),
            buttons: Default::default(),
            slots: root.slots.clone(),
            frames: root.frames.clone(),
        };
        rewards(v, &only, q, scene, o, s, None);
        v.button(root, "ok_btn", o, s, Action::ClaimQuest(q.id));
        v.button(root, "close_btn", o, s, Action::Close);
        return true;
    }
    let (o, s) = centered(camera, root.size);
    v.add(
        root,
        o,
        s,
        |n| {
            !n.parents
                .iter()
                .any(|p| p == "ok_btn" || p == "bottom_gradient" || p == "top_gradient")
        },
        |n| {
            Some(if n.id == "header_txt" {
                q.name.clone()
            } else {
                String::new()
            })
        },
        None,
    );
    for d in &mut v.draws {
        if d.node.parents.iter().any(|p| p == "pointer") {
            d.node.rect[1] += index as f64 * 52. * s;
        }
    }
    quest_badges(v, scene, state, camera, true);
    let top = &c.symbols["quests_top"];
    let row = &c.symbols["quests_item"];
    let bottom = &c.symbols["quests_bottom"];
    // QuestDialog.init/updateGUI0x103a61/0x104225 replaces authored template
    // heights after MTLabel.calcSize. The scroll frame is53,43,417,263.
    let clip = [(o[0] + 53.) * s, (o[1] + 43.) * s, 417. * s, 263. * s];
    let desc = interface::lang(scene, &format!("#QUEST_{}_TEXT", q.id), "");
    let title = top.nodes.iter().find(|n| n.id == "captiontop_txt").unwrap();
    let title_h = crate::interface_text::candara_height(title, desc);
    let header_h = title_h.max(40.);
    let mut rows = Vec::new();
    for e in &q.exercises {
        let mut symbol = row.clone();
        let description = exercise_copy(scene, e, &e.text);
        let hint = exercise_copy(scene, e, &e.hint);
        let description_h = crate::interface_text::candara_height(
            symbol
                .nodes
                .iter()
                .find(|n| n.id == "description_txt")
                .unwrap(),
            &description,
        );
        let hint_h = crate::interface_text::candara_height(
            symbol
                .nodes
                .iter()
                .find(|n| n.id == "hinttext_txt")
                .unwrap(),
            &hint,
        );
        let hint_y = 4. + description_h + 5.;
        let hinttext_y = hint_y + 12.;
        let height = if hint.is_empty() {
            (4. + description_h + 5.).max(68.)
        } else {
            (hinttext_y + hint_h + 5.).max(68.)
        };
        for n in &mut symbol.nodes {
            match n.id.as_str() {
                "bkg" => n.rect[3] = height,
                "description_txt" => {
                    n.rect[1] = 4.;
                    n.rect[3] = description_h + 5.;
                }
                "hint_txt" => n.rect[1] = hint_y,
                "hinttext_txt" => {
                    n.rect[1] = hinttext_y;
                    n.rect[3] = hint_h + 5.;
                }
                "complete_icon" => n.rect[1] = ((height - n.rect[3]) / 2.).round(),
                _ => {}
            }
        }
        for id in ["visit_btn", "buy_btn"] {
            // QuestDialog.updateGUI places the lone visible visit button at
            // y5; only complete_icon is centred vertically in the exercise.
            let r = symbol.buttons[id];
            let dy = 5. - r[1];
            symbol.buttons.get_mut(id).unwrap()[1] += dy;
            for n in &mut symbol.nodes {
                if n.parents.iter().any(|p| p == id) {
                    n.rect[1] += dy;
                }
            }
        }
        rows.push((symbol, height));
    }
    let total = header_h + 12. + rows.iter().map(|(_, h)| h + 3.).sum::<f64>() + bottom.size[1];
    v.scroll_max = (total - 263.).max(0.) * s;
    let offset = scroll / s;
    v.add(
        top,
        [
            o[0] + 90.,
            o[1] + 43. + ((header_h - title_h) / 2.).round() - offset,
        ],
        s,
        |_| true,
        |_| Some(desc.into()),
        Some(clip),
    );
    let mut row_y = o[1] + 43. + header_h + 12. - offset;
    for (i, e) in q.exercises.iter().enumerate() {
        let (row, height) = &rows[i];
        let origin = [o[0] + 63., row_y];
        row_y += height + 3.;
        let (count, ready) = state.exercise_progress(scene, q, i);
        let show = crate::quest_navigation::visible(scene, state, q.id, i);
        let row_begin = v.draws.len();
        v.add(
            row,
            origin,
            s,
            |n| {
                !n.parents
                    .iter()
                    .any(|p| p == "buy_btn" || (p == "visit_btn" && !show))
                    && (n.id != "complete_icon" || ready)
                    && (!e.hint.is_empty() || !n.id.starts_with("hint"))
            },
            |n| {
                Some(match n.id.as_str() {
                    "show_btn_txt" => interface::lang(scene, "#show_button", "Показать").into(),
                    "description_txt" => {
                        if e.text.starts_with('#') {
                            String::new()
                        } else {
                            exercise_copy(scene, e, &e.text)
                        }
                    }
                    "counter_txt" => format!("{}/{}", count.min(e.value), e.value),
                    "hint_txt" => {
                        format!(
                            "{}:",
                            interface::lang(scene, "#DIALOG_HINT1_HEADER", "Подсказка")
                                .trim_end_matches(':')
                        )
                    }
                    "hinttext_txt" => exercise_copy(scene, e, &e.hint),
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        if show {
            let start = v.hits.len();
            v.button(row, "visit_btn", origin, s, Action::ShowQuest(q.id, i));
            // The clipped list must not leave invisible interactive rows.
            for hit in &mut v.hits[start..] {
                let [x, y, w, h] = hit.rect;
                let right = (x + w).min(clip[0] + clip[2]);
                let bottom = (y + h).min(clip[1] + clip[3]);
                hit.rect = [
                    x.max(clip[0]),
                    y.max(clip[1]),
                    (right - x.max(clip[0])).max(0.),
                    (bottom - y.max(clip[1])).max(0.),
                ];
            }
        }
        if let Some(icon) = c.exercise_icons.get(&e.icon) {
            v.add(
                icon,
                [origin[0] + 45.8, origin[1] + 32.825],
                s,
                |_| true,
                |_| None,
                Some(clip),
            );
            // QuestDialog.updateGUI adds the generated icon INSIDE icon_img.
            // counter_txt is a later sibling, so it stays above that subtree.
            // Flattening the generated icon after the complete row hid counts
            // behind large custom artwork, notably collect_pvp_playmoney.
            if let Some(index) = v.draws[row_begin..]
                .iter()
                .position(|d| d.node.id == "counter_txt")
            {
                let counter = v.draws.remove(row_begin + index);
                v.draws.push(counter);
            }
        }
    }
    let reward_begin = v.draws.len();
    rewards(v, bottom, q, scene, [o[0] + 83.5, row_y], s, Some(clip));
    // QuestDialog.updateGUI0x104225 centres this label at scrollWidth/2-7,
    // independently of the authored331pt rewards template's placeholder x.
    for d in &mut v.draws[reward_begin..] {
        if d.node.id == "rewards_caption_txt" {
            d.node.rect[0] = clip[0] + clip[2] * 0.5 - 7. * s - d.node.rect[2] * 0.5;
        }
    }
    v.button(root, "close_btn", o, s, Action::Close);
    true
}
