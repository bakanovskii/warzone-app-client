//! Recovered ShopDialog root, tabs, cards, clipping and source currency groups.
use crate::{
    Camera, Scene,
    interface::{self, Action, Draw, View},
    production::State,
};
pub fn build(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    tab: u32,
    scroll: f64,
) -> bool {
    let Some(c) = &scene.interface else {
        return false;
    };
    let Some(root) = c.symbols.get("shop") else {
        return false;
    };
    if scene.shop.is_none() || !(1..=7).contains(&tab) {
        return false;
    }
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    let s = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / s - root.size[0]) * 0.5,
        (camera.height as f64 / s - root.size[1]) * 0.5,
    ];
    let header = [
        "",
        "#BANK",
        "#ADMINISTRATIVE_PAGE",
        "#INDUSTRIAL_PAGE",
        "#DEFENSIVE_PAGE",
        "#decour_page",
        "#UNITS_PAGE",
        "#ITEMS_PAGE",
    ][tab as usize];
    let bank = tab == 1 && crate::bank::available(scene);
    let (used, max) = state.population(scene);
    v.add(
        root,
        o,
        s,
        |n| {
            if n.parents.iter().any(|p| {
                ["cash_factor", "scroll_side_left", "scroll_side_right"].contains(&p.as_str())
            }) {
                return false;
            }
            if n.parents.iter().any(|p| p == "layerShopBank") {
                return bank;
            }
            for p in &n.parents {
                if let Some(id) = p.strip_prefix("activeTab") {
                    if id.parse::<u32>().ok() != Some(tab) {
                        return false;
                    }
                }
                if let Some(id) = p.strip_prefix("tab") {
                    if let Some(id) = id.strip_suffix("_big") {
                        if id.parse::<u32>().ok() != Some(tab) {
                            return false;
                        }
                    } else if id.parse::<u32>().ok() == Some(tab) {
                        return false;
                    }
                }
            }
            n.id != "no_internet_txt" || (tab == 1 && !bank)
        },
        |n| {
            Some(match n.id.as_str() {
                "header_txt" => interface::lang(scene, header, "").into(),
                "hud_capacity_txt" => format!("{used}/{max}"),
                "hud_real_money_txt" => state.resources["realMoney"].to_string(),
                "hud_play_money_txt" => state.resources["playMoney"].to_string(),
                "hud_oil_txt" => state.resources["fuel"].to_string(),
                "hud_iron_txt" => state.resources["iron"].to_string(),
                "no_internet_txt" => interface::lang(
                    scene,
                    "#bank_load_error",
                    "Не удается загрузить банк, пожалуйста, попробуйте позже.",
                )
                .into(),
                _ => String::new(),
            })
        },
        None,
    );
    v.button(root, "close_btn", o, s, Action::Close);
    for t in 1..=7 {
        v.button(
            root,
            &format!("tab{t}{}", if t == tab { "_big" } else { "" }),
            o,
            s,
            Action::ShopTab(t),
        );
    }
    if tab == 1 {
        if bank {
            crate::interface_bank::build(v, scene, state, camera, o, scroll);
        }
        return true;
    }
    let left = root.frames["scroll_side_left"];
    let right = root.frames["scroll_side_right"];
    let clip = [
        (o[0] + left[0]) * s,
        (o[1] + left[1]) * s,
        (right[0] + right[2] - left[0]) * s,
        left[3] * s,
    ];
    let items = state.source_shop_items(scene, tab);
    let content = 5. + items.len() as f64 * 130.;
    v.scroll_max = (content * s - clip[2]).max(0.);
    let scroll = scroll / s;
    for (index, item) in items.iter().enumerate() {
        let locked = state.shop_locked(item);
        let maximum = item.max_count(35);
        let count = state.building_count(item.id);
        let access = state.shop_access(item).min(maximum);
        let o = [
            o[0] + left[0] + 5. + 62.5 + index as f64 * 130. - scroll,
            o[1] + left[1] + 5.,
        ];
        if (o[0] + 62.5) * s < clip[0] || (o[0] - 62.5) * s > clip[0] + clip[2] {
            continue;
        }
        let Some(template) = c.symbols.get(if locked {
            "shop_item_locked"
        } else {
            "shop_item_normal"
        }) else {
            continue;
        };
        let mut name = scene.item_name(item.id, &item.name).to_string();
        if item.kind == "special_items" {
            if let Some(i) = scene
                .progression
                .as_ref()
                .and_then(|p| p.special_items.get(&item.id))
            {
                if let Some(a) = i
                    .outputs
                    .iter()
                    .find(|a| ["playMoney", "fuel", "iron"].contains(&a.kind.as_str()))
                {
                    let key = match a.kind.as_str() {
                        "playMoney" => "#amount_playmoney",
                        "fuel" => "#amount_fuel",
                        _ => "#amount_iron",
                    };
                    name = crate::interface_resources::number_text(
                        interface::lang(scene, key, &i.name),
                        a.count.parse().unwrap_or(0),
                    );
                } else {
                    name = scene.item_name(item.id, &i.name).into()
                }
            }
        }
        let need = item
            .next_level(count)
            .unwrap_or(item.min_level)
            .max(item.min_level);
        let locktext = if count >= maximum {
            interface::lang(scene, "#max_building_limit", "").to_string()
        } else if item.research != 0
            && !state
                .systems
                .as_ref()
                .is_some_and(|p| p.flag(&format!("Research-{}", item.research)))
        {
            interface::lang(scene, &format!("#research_{}", item.research), "").into()
        } else {
            crate::interface_resources::number_text(
                interface::lang(scene, "#NEED_LEVEL", "[1] уровень"),
                need,
            )
        };
        let seconds = scene
            .construction
            .as_ref()
            .and_then(|c| c.buildings.get(&item.id))
            .map_or(0, |b| b.seconds(count));
        let stat =
            item.kind == "units" || [1021, 1022, 1036, 3001, 3002, 3010, 3011].contains(&item.id);
        let defense = [1021, 1022, 1036].contains(&item.id);
        let wall = [3001, 3002, 3010, 3011].contains(&item.id);
        let ability = match item.ability {
            11 => "x2damage",
            10 => "knife",
            7 => "armor",
            _ => "",
        };
        let start = v.draws.len();
        v.add(
            template,
            o,
            s,
            |n| {
                if n.parents.iter().any(|p| p == "ability") {
                    return !ability.is_empty() && n.parents.iter().any(|p| p == ability);
                }
                if n.parents.iter().any(|p| p == "weapon_icons") {
                    if !stat {
                        return false;
                    }
                    if wall {
                        return n.id.starts_with("defence_");
                    }
                    if defense && n.id.starts_with("speed_") {
                        return false;
                    }
                }
                if n.parents.iter().any(|p| p == "symbToken") {
                    return item.kind == "units";
                }
                if ["build_time_icon", "build_time_txt"].contains(&n.id.as_str()) {
                    return tab < 5 && seconds > 0 && access < 65535 && !locked;
                }
                if n.id == "count_txt" {
                    return access < 65535 && (count != 0 || access != 0);
                }
                if n.parents.iter().any(|p| p == "buy_btnLocked") {
                    return locked && count >= maximum;
                }
                if n.parents.iter().any(|p| p == "buy_btn") {
                    return !locked || count < maximum;
                }
                if n.id == "lock_limit_txt" {
                    return locked && count >= maximum;
                }
                if n.id == "lock_txt" {
                    return locked && count < maximum;
                }
                for (prefix, key) in [
                    ("play_money_", "playMoney"),
                    ("real_money_", "realMoney"),
                    ("oil_", "fuel"),
                    ("iron_", "iron"),
                ] {
                    if n.id.starts_with(prefix) {
                        return item.cost.get(key).is_some_and(|n| *n > 0);
                    }
                }
                true
            },
            |n| {
                Some(match n.id.as_str() {
                    "caption_txt" => name.clone(),
                    "count_txt" => format!("{count}/{access}"),
                    "lock_txt" | "lock_limit_txt" => locktext.clone(),
                    "build_time_txt" => crate::interface_resources::duration(scene, seconds),
                    "button_txt" => {
                        interface::lang(scene, if locked { "#unlock_btn" } else { "#btn_buy" }, "")
                            .into()
                    }
                    "btn_real_money_txt" => item.unlock_price(count).unwrap_or(0).to_string(),
                    "defence_txt" => item.stats.get("health").unwrap_or(&0).to_string(),
                    "attack_txt" => item.stats.get("attack").unwrap_or(&0).to_string(),
                    "range_txt" => item.stats.get("distance").unwrap_or(&0).to_string(),
                    "speed_txt" => item.stats.get("speed").unwrap_or(&0).to_string(),
                    "token_txt" => item.stats.get("population").unwrap_or(&0).to_string(),
                    "play_money_txt" => item.cost.get("playMoney").unwrap_or(&0).to_string(),
                    "real_money_txt" => item.cost.get("realMoney").unwrap_or(&0).to_string(),
                    "oil_txt" => item.cost.get("fuel").unwrap_or(&0).to_string(),
                    "iron_txt" => item.cost.get("iron").unwrap_or(&0).to_string(),
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        for d in &mut v.draws[start..] {
            // Repeated template IDs are local to each source UIView instance.
            // Retain that identity for shared label/icon button layout.
            d.node.parents.insert(0, format!("shop_item_{}", item.id));
            if d.node.parents.iter().any(|p| p == "buy_btnLocked") {
                if d.node.kind == "image" {
                    d.grayscale = true;
                }
                if d.node.kind == "text" {
                    // This source button has a nonzero frame pivot. Its bkg
                    // already includes it; textFields are local to that frame.
                    let frame = template.buttons["buy_btnLocked"];
                    let slot = template.slots["buy_btnLocked"];
                    d.node.rect[0] += (frame[0] - slot[0]) * s;
                    d.node.rect[1] += (frame[1] - slot[1]) * s;
                }
            }
            if d.node.parents.iter().any(|p| p == "weapon_icons") {
                d.node.rect[0] += if wall {
                    37. * s
                } else if defense {
                    12. * s
                } else {
                    0.
                };
            }
        }
        // Source dynamically centers only the nonzero currencies over the buy
        // button, in playMoney/fuel/iron/realMoney order (10pxphone spacing group).
        let currencies: Vec<_> = [
            ("play_money", "playMoney"),
            ("oil", "fuel"),
            ("iron", "iron"),
            ("real_money", "realMoney"),
        ]
        .into_iter()
        .filter(|(_, k)| item.cost.get(*k).is_some_and(|n| *n > 0))
        .collect();
        let step = match currencies.len() {
            2 => 30.,
            3 => 25.,
            _ => 22.,
        };
        for (i, (prefix, key)) in currencies.iter().enumerate() {
            let center = o[0] + 1. - step * (currencies.len().saturating_sub(1)) as f64 * 0.5
                + step * i as f64;
            for d in &mut v.draws[start..] {
                if d.node.id == format!("{prefix}_icon") || d.node.id == format!("{prefix}_txt") {
                    d.node.rect[0] = center * s - d.node.rect[2] * 0.5;
                    if d.node.kind == "text" {
                        d.node.color = if count >= maximum {
                            0x808080
                        } else if state.resources.get(*key).copied().unwrap_or(0) < item.cost[*key]
                        {
                            0xff0000
                        } else {
                            d.node.color
                        };
                    }
                }
            }
        }
        // Insert the original UIImage below the source lock overlay/labels.
        let r = template.frames["entity_image"];
        let info = &scene.textures[&item.thumbnail];
        let (w, h) = (info.width as f64 * 0.5, info.height as f64 * 0.5);
        let mut n = root.nodes[0].clone();
        n.id = format!("shop_thumb_{}", item.id);
        n.texture = item.thumbnail.clone();
        n.parents = vec![format!("shop_item_{}", item.id)];
        n.rect = [
            (o[0] + r[0] + (r[2] - w) * 0.5) * s,
            (o[1]
                + r[1]
                + (r[3] - h) * 0.5
                + if defense || item.kind == "units" {
                    10.
                } else {
                    0.
                })
                * s,
            w * s,
            h * s,
        ];
        let at = (start..v.draws.len())
            .find(|i| {
                ["lock_icon", "count_txt", "build_time_icon", "ability"]
                    .contains(&v.draws[*i].node.id.as_str())
            })
            .unwrap_or(v.draws.len());
        v.draws.insert(
            at,
            Draw {
                node: n,
                clip: Some(clip),
                progress: 1.,
                opacity: 255,
                grayscale: locked,
            },
        );
        if count < maximum {
            let begin = v.hits.len();
            v.button(
                template,
                "buy_btn",
                o,
                s,
                if locked {
                    Action::ShopUnlock(item.id)
                } else {
                    Action::ShopBuy(item.id)
                },
            );
            for h in &mut v.hits[begin..] {
                let x = h.rect[0].max(clip[0]);
                let end = (h.rect[0] + h.rect[2]).min(clip[0] + clip[2]);
                h.rect[0] = x;
                h.rect[2] = (end - x).max(0.);
            }
            v.hits.retain(|h| h.rect[2] > 0.);
        }
    }
    true
}
