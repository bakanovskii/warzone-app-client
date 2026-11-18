//! ProductionDialog's four original contract slots and level-access prompt.
use crate::{
    Camera, Scene,
    interface::{self, Action, Draw, View},
    production::{State, number},
};

/// Original Russian one-parameter localization syntax, used by this source
/// dialog. Preserve both [1] substitutions and the three plural alternatives.
pub fn number_text(text: &str, n: i64) -> String {
    let text = text.replace("[1]", &n.to_string());
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("[1|") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 3..];
        let Some(end) = tail.find(']') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let forms: Vec<_> = tail[..end].split('|').collect();
        if forms.len() == 2 {
            let index = usize::from(n.unsigned_abs() != 1);
            out.push_str(&forms[index].replace('#', &n.to_string()));
        } else if forms.len() >= 3 {
            let n = n.unsigned_abs();
            let index = if (11..=14).contains(&(n % 100)) {
                2
            } else {
                match n % 10 {
                    1 => 0,
                    2..=4 => 1,
                    _ => 2,
                }
            };
            out.push_str(&forms[index].replace('#', &n.to_string()));
        } else {
            out.push_str(&rest[start..start + 3 + end + 1]);
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}
pub(crate) fn duration(scene: &Scene, seconds: i64) -> String {
    let (key, fallback, count) = if seconds < 60 {
        ("#secs_short", "[1] сек.", seconds)
    } else if seconds < 3600 {
        ("#minutes_short", "[1] мин.", seconds / 60)
    } else {
        ("#hours_short", "[1] ч.", seconds / 3600)
    };
    number_text(interface::lang(scene, key, fallback), count)
}
pub fn contracts(v: &mut View, scene: &Scene, state: &State, camera: &Camera, uid: &str) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(root) = c.symbols.get("resource_production") else {
        return false;
    };
    let Some(e) = state.entities.get(uid) else {
        return false;
    };
    let Some(ec) = &scene.economy else {
        return false;
    };
    let Some(b) = ec
        .buildings
        .get(&(number(e, "type") as u32))
        .filter(|b| !b.recipes.is_empty())
    else {
        return false;
    };
    if number(e, "productionType") != 0 {
        return false;
    }
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    let scale = crate::hud::scale(camera);
    let origin = [
        (camera.width as f64 / scale - root.size[0]) * 0.5,
        (camera.height as f64 / scale - root.size[1]) * 0.5,
    ];
    v.add(
        root,
        origin,
        scale,
        |n| !n.parents.iter().any(|p| p.starts_with("slot")),
        |n| {
            Some(match n.id.as_str() {
                "caption_txt" => scene.item_name(number(e, "type") as u32, "").into(),
                "hud_real_money_txt" => state.resources["realMoney"].to_string(),
                "hud_play_money_txt" => state.resources["playMoney"].to_string(),
                "hud_oil_txt" => state.resources["fuel"].to_string(),
                "hud_iron_txt" => state.resources["iron"].to_string(),
                _ => String::new(),
            })
        },
        None,
    );
    for (i, id) in b.recipes.iter().take(4).enumerate() {
        let Some(r) = ec.recipes.get(id) else {
            continue;
        };
        let key = format!("slot{}", i + 1);
        let Some(template) = c.symbols.get(&format!("resource_production_{key}")) else {
            continue;
        };
        let slot = &root.slots[&key];
        let o = [origin[0] + slot[0], origin[1] + slot[1]];
        let locked = state.needs_recipe_unlock(scene, *id);
        let output = ["iron", "fuel", "playMoney"]
            .into_iter()
            .find(|k| r.outputs.get(*k).is_some_and(|r| r.count != 0))
            .unwrap_or("playMoney");
        let outicon = match output {
            "iron" => "res_iron0",
            "fuel" => "res_oil0",
            _ => "res_money0",
        };
        let money = r.cost.get("playMoney").copied().unwrap_or(0);
        let fuel = r.cost.get("fuel").copied().unwrap_or(0);
        let start = v.draws.len();
        v.add(
            template,
            o,
            scale,
            |n| {
                if n.parents.iter().any(|p| p == "btn_Build0") {
                    return !locked;
                }
                if n.parents.iter().any(|p| p == "btn_unlock0") {
                    return locked;
                }
                if n.parents.iter().any(|p| p == "require_group0") {
                    return locked;
                }
                if n.parents.iter().any(|p| p == "price_group0") {
                    return !locked
                        && match n.id.as_str() {
                            "need_money0" => money != 0,
                            "need_oil0" => money == 0 && fuel != 0,
                            _ => money != 0 || fuel != 0,
                        };
                }
                if n.id.starts_with("res_") {
                    return n.id
                        == if locked {
                            format!("{outicon}_gray")
                        } else {
                            outicon.into()
                        };
                }
                true
            },
            |n| {
                Some(match n.id.as_str() {
                    "name0_txt" => interface::lang(scene, &format!("#recipe_{id}"), "").into(),
                    "time0_txt" => duration(scene, r.seconds),
                    "produce0_txt" => interface::lang(
                        scene,
                        if locked {
                            "#unlock_btn"
                        } else {
                            "#btn_produce"
                        },
                        if locked {
                            "Доступ"
                        } else {
                            "Производить"
                        },
                    )
                    .into(),
                    "requirement0_txt" => number_text(
                        interface::lang(scene, "#NEED_LEVEL", "[1] уровень"),
                        r.min_level,
                    ),
                    "require0_txt" => interface::lang(scene, "#required", "Требуется").into(),
                    "price0_txt" => if money != 0 { money } else { fuel }.to_string(),
                    "res0_txt" => r.outputs.get(output).map_or(0, |r| r.count).to_string(),
                    _ => String::new(),
                })
            },
            None,
        );
        if let Some(texture) = c.resource_icons.get(id) {
            // ProductionDialog sets UIViewContentModeCenter (4), not aspect fit.
            // Its UIImageView has even-rounded placeholder dimensions; phone
            // UIImage scale=2 preserves the image's natural logical size.
            let slot = &template.slots["imagePlace0"];
            let info = &scene.textures[texture];
            let container_w = (slot[2] as i64 / 2 * 2) as f64;
            let container_h = (slot[3] as i64 / 2 * 2) as f64;
            let (w, h) = (info.width as f64 * 0.5, info.height as f64 * 0.5);
            let mut n = template
                .nodes
                .iter()
                .find(|n| n.kind == "image")
                .unwrap()
                .clone();
            n.id = format!("recipe_icon_{id}");
            n.parents.clear();
            n.texture = texture.clone();
            n.rect = [
                (o[0] + slot[0] + (container_w - w) * 0.5) * scale,
                (o[1] + slot[1] + (container_h - h) * 0.5) * scale,
                w * scale,
                h * scale,
            ];
            v.draws.insert(
                start + 1,
                Draw {
                    node: n,
                    clip: None,
                    progress: 1.,
                    opacity: 255,
                    grayscale: locked,
                },
            );
        }
        v.button(
            template,
            if locked { "btn_unlock0" } else { "btn_Build0" },
            o,
            scale,
            if locked {
                Action::RecipeAccess(*id)
            } else {
                Action::Produce(i)
            },
        );
    }
    v.button(root, "close_btn", origin, scale, Action::Close);
    true
}

pub fn speedup(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    uid: &str,
    now: i64,
) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(root) = c.symbols.get("production_speedup") else {
        return false;
    };
    let Some(ec) = &scene.economy else {
        return false;
    };
    let Some(wall) = now.checked_sub(ec.time_shift) else {
        return false;
    };
    let Ok(q) = state.resource_speedup_quote(scene, uid, wall) else {
        return false;
    };
    let r = &ec.recipes[&q.recipe];
    let scale = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / scale - root.size[0]) * 0.5,
        (camera.height as f64 / scale - root.size[1]) * 0.5,
    ];
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.popup_shade(&root.nodes[0], camera);
    let out = ["iron", "fuel", "playMoney"]
        .into_iter()
        .find(|k| r.outputs.get(*k).is_some_and(|r| r.count != 0))
        .unwrap_or("playMoney");
    let icon = match out {
        "iron" => "iron_icon",
        "fuel" => "oil_icon",
        _ => "money_icon",
    };
    v.add(
        root,
        o,
        scale,
        |n| {
            if n.id == "bottomTileImg" {
                // SpeedupDialog.updateGUIRecipes hides the building's island.
                false
            } else if ["iron_icon", "oil_icon", "money_icon"].contains(&n.id.as_str()) {
                n.id == icon
            } else {
                true
            }
        },
        |n| {
            Some(match n.id.as_str() {
                "header_txt" => interface::lang(scene, "#in_production", "Производство").into(),
                "name_txt" => interface::lang(scene, &format!("#recipe_{}", q.recipe), "").into(),
                "timelefthint_txt" => {
                    interface::lang(scene, "#production_time_left", "Осталось времени").into()
                }
                "producenow_txt" => {
                    interface::lang(scene, "#instant_finish", "Завершить мгновенно").into()
                }
                // [1] is the localization's optional inline-price slot. The
                // recovered popup puts its icon/price in separate native fields.
                "buybtn_txt" => interface::lang(scene, "#speed_up_button", "Ускорить")
                    .replace("[1]", "")
                    .trim()
                    .into(),
                "buyprice_txt" => q.cash.to_string(),
                "value_txt" => r.outputs.get(out).map_or(0, |r| r.count).to_string(),
                "timer_txt" => {
                    if q.remaining >= 3600 {
                        format!(
                            "{}:{:02}:{:02}",
                            q.remaining / 3600,
                            q.remaining / 60 % 60,
                            q.remaining % 60
                        )
                    } else {
                        format!("{}:{:02}", q.remaining / 60, q.remaining % 60)
                    }
                }
                _ => String::new(),
            })
        },
        None,
    );
    if let Some(texture) = c.resource_icons.get(&q.recipe) {
        let rect = &root.frames["IconImage"];
        let info = &scene.textures[texture];
        // Same UIImageView center mode as the source recipe selector.
        let (w, h) = (info.width as f64 * 0.5, info.height as f64 * 0.5);
        let mut n = root.nodes[0].clone();
        n.id = format!("active_recipe_{}", q.recipe);
        n.texture = texture.clone();
        n.parents.clear();
        n.rect = [
            (o[0] + rect[0] + (rect[2] - w) * 0.5) * scale,
            (o[1] + rect[1] + (rect[3] - h) * 0.5) * scale,
            w * scale,
            h * scale,
        ];
        // Keep the icon behind output value/currency/close controls.
        let at = v
            .draws
            .iter()
            .position(|d| d.node.id == "value_txt")
            .unwrap_or(v.draws.len());
        v.draws.insert(
            at,
            Draw {
                node: n,
                clip: None,
                progress: 1.,
                opacity: 255,
                grayscale: false,
            },
        );
    }
    v.button(root, "speedup_btn", o, scale, Action::ResourceSpeedup);
    v.button(root, "close_btn", o, scale, Action::Close);
    true
}

/// Same recovered SpeedupDialog, with its original research/training/item modes.
pub fn active_speedup(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    uid: &str,
    now: i64,
) -> bool {
    use crate::production_speedup::Kind;
    let Some(ec) = &scene.economy else {
        return false;
    };
    let Some(wall) = now.checked_sub(ec.time_shift) else {
        return false;
    };
    let Ok(q) = state.production_speedup_quote(scene, uid, wall) else {
        return false;
    };
    if q.kind == Kind::Resource {
        return speedup(v, scene, state, camera, uid, now);
    }
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(root) = c.symbols.get("production_speedup") else {
        return false;
    };
    let (header, name, thumbnail, material) = if q.kind == Kind::Construction {
        (
            "#in_construction",
            scene.item_name(q.recipe, "").to_owned(),
            c.building_thumbs.get(&q.recipe),
            None,
        )
    } else if q.kind == Kind::Upgrade {
        let r = &scene.upgrades.as_ref().unwrap().items[&q.recipe];
        (
            "#in_upgrade",
            interface::lang(scene, &format!("#upgrade_{}", q.recipe), &r.name).to_string(),
            None,
            c.symbols.get(&format!("upgrade_large_{}", q.recipe)),
        )
    } else if q.kind == Kind::Research {
        let r = &scene.research.as_ref().unwrap().items[&q.recipe];
        (
            "#in_research",
            interface::lang(scene, &format!("#research_{}", q.recipe), &r.name).to_string(),
            c.unit_thumbs.get(&r.unit),
            None,
        )
    } else {
        let r = &scene.workshop.as_ref().unwrap().recipes[&q.recipe];
        (
            if r.unit() {
                "#in_training"
            } else {
                "#in_production"
            },
            if r.unit() {
                scene.item_name(r.output, &r.name)
            } else {
                interface::lang(scene, &format!("#recipe_{}", q.recipe), &r.name)
            }
            .to_string(),
            if r.unit() {
                c.unit_thumbs.get(&r.output)
            } else {
                None
            },
            if r.unit() {
                None
            } else {
                c.symbols
                    .get(&format!("speedup_item_{}", r.output))
                    .or_else(|| c.item_icons.get(&r.output))
            },
        )
    };
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    let scale = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / scale - root.size[0]) * 0.5,
        (camera.height as f64 / scale - root.size[1]) * 0.5,
    ];
    v.popup_shade(&root.nodes[0], camera);
    v.add(
        root,
        o,
        scale,
        |n| match n.id.as_str() {
            "value_txt" | "iron_icon" | "oil_icon" | "money_icon" => false,
            "bottomTileImg" => q.kind == Kind::Research,
            _ => true,
        },
        |n| {
            Some(match n.id.as_str() {
                "header_txt" => interface::lang(scene, header, "").into(),
                "name_txt" => name.clone(),
                "timelefthint_txt" => {
                    interface::lang(scene, "#production_time_left", "Осталось времени").into()
                }
                "producenow_txt" => {
                    interface::lang(scene, "#instant_finish", "Завершить мгновенно").into()
                }
                "buybtn_txt" => interface::lang(scene, "#speed_up_button", "Ускорить")
                    .replace("[1]", "")
                    .trim()
                    .into(),
                "buyprice_txt" => q.cash.to_string(),
                "timer_txt" => {
                    if q.remaining >= 3600 {
                        format!(
                            "{}:{:02}:{:02}",
                            q.remaining / 3600,
                            q.remaining / 60 % 60,
                            q.remaining % 60
                        )
                    } else {
                        format!("{}:{:02}", q.remaining / 60, q.remaining % 60)
                    }
                }
                _ => String::new(),
            })
        },
        None,
    );
    let rect = &root.frames["IconImage"];
    if let Some(texture) = thumbnail {
        let info = &scene.textures[texture];
        // GraphicsManager.loadUIImageFromFile uses scale2 on phones, and
        // SpeedupDialog uses UIViewContentModeCenter (4), not aspect-fit.
        let (w, h) = (info.width as f64 * 0.5, info.height as f64 * 0.5);
        let mut n = root.nodes[0].clone();
        n.id = format!("active_work_{}", q.recipe);
        n.texture = texture.clone();
        n.parents.clear();
        n.rect = [
            (o[0] + rect[0] + (rect[2] - w) * 0.5) * scale,
            (o[1] + rect[1] + (rect[3] - h) * 0.5) * scale,
            w * scale,
            h * scale,
        ];
        v.draws.push(Draw {
            node: n,
            clip: None,
            progress: 1.,
            opacity: 255,
            grayscale: false,
        });
    }
    if let Some(icon) = material {
        // Source setCenterToPoint centers the generated UIView. Exported SP
        // nodes still have their native pivot, rather than UIKit's zero origin.
        let min_x = icon
            .nodes
            .iter()
            .map(|n| n.rect[0])
            .fold(f64::INFINITY, f64::min);
        let min_y = icon
            .nodes
            .iter()
            .map(|n| n.rect[1])
            .fold(f64::INFINITY, f64::min);
        let max_x = icon
            .nodes
            .iter()
            .map(|n| n.rect[0] + n.rect[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = icon
            .nodes
            .iter()
            .map(|n| n.rect[1] + n.rect[3])
            .fold(f64::NEG_INFINITY, f64::max);
        v.add(
            icon,
            [
                o[0] + rect[0] + rect[2] * 0.5 - (min_x + max_x) * 0.5,
                o[1] + rect[1] + rect[3] * 0.5 - 5. - (min_y + max_y) * 0.5,
            ],
            scale,
            |_| true,
            |_| None,
            None,
        );
    }
    v.button(root, "speedup_btn", o, scale, Action::ResourceSpeedup);
    v.button(root, "close_btn", o, scale, Action::Close);
    true
}
