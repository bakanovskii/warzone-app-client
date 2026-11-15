//! Unit.createTooltipMini / moveTooltip::: (0xc5d49). Source HUD images
//! remain screen-sized; only iconMargin(-20) follows the map zoom.
use crate::{Camera, Scene, interface::View};
pub fn scale(camera: &Camera) -> f64 {
    let hud = crate::hud::scale(camera);
    hud * (camera.zoom / (hud * 0.5)).min(1.)
}
pub fn available(scene: &Scene) -> bool {
    scene
        .interface
        .as_ref()
        .is_some_and(|c| c.symbols.contains_key("unit_health"))
}
pub fn add(
    view: &mut View,
    scene: &Scene,
    camera: &Camera,
    position: [f64; 2],
    hp: f64,
    max: f64,
) -> Option<()> {
    let symbol = scene.interface.as_ref()?.symbols.get("unit_health")?;
    let scale = scale(camera);
    view.retained_unit_health = true;
    let start = view.draws.len();
    view.add(
        symbol,
        [
            position[0] / scale,
            (position[1] - 20. * camera.zoom) / scale,
        ],
        scale,
        |_| true,
        |_| None,
        None,
    );
    for d in &mut view.draws[start..] {
        d.node.parents.push("source_unit_health".into());
        if d.node.id == "progressbar" {
            d.progress = if max > 0. {
                (hp / max).clamp(0., 1.)
            } else {
                0.
            };
        }
    }
    Some(())
}
/// createTooltipMini:NO retains the original extended symbol. Statistics are
/// live upgraded unit fields; a damaged building/obstacle is never offered here.
pub fn full(
    view: &mut View,
    scene: &Scene,
    camera: &Camera,
    position: [f64; 2],
    u: &crate::battle::Unit,
    hp: f64,
) -> Option<()> {
    if matches!(u.kind.as_str(), "building" | "decor") {
        return building(view, scene, camera, position, u, hp);
    }
    let c = scene.interface.as_ref()?;
    let symbol = c.symbols.get("unit_info")?;
    let ability = match u.ability {
        10 => Some(("knife", "#ability_10")),
        11 => Some(("x2damage", "#ability_2")),
        7 => Some(("armor", "#ability_7")),
        _ => None,
    };
    let scale = scale(camera);
    let start = view.draws.len();
    // Unit.updateTooltipForZoom uses0.5 on the low-performance phone profile
    // (literal0xc5c5c), versus0.3 on high-performance devices. Clamp only for
    // the desktop's extra zoom range, which extends below the original minimum.
    let zoom = camera.zoom * 0.5;
    let t = ((zoom - 0.5)
        / (if view.retained_battle_selection {
            0.75
        } else {
            0.7
        } - 0.5))
        .clamp(0., 1.);
    let health = symbol
        .frames
        .get("health_ext")
        .copied()
        .unwrap_or([-41., -17., 0., 0.]);
    let attack = symbol
        .frames
        .get("attack_ext")
        .copied()
        .unwrap_or([-57.5, 13., 0., 0.]);
    let ability_rect = symbol
        .frames
        .get("ability_ext")
        .copied()
        .unwrap_or([-39.5, 42., 0., 0.]);
    let attack_at = [
        ((health[0] - 5. - 10. * t) * 2.).round() / 2.,
        ((health[1] + 32.5 * (0.7 + 0.3 * t)) * 2.).round() / 2.,
    ];
    let ability_y = ((health[1] + 65. * (0.7 + 0.3 * t)) * 2.).round() / 2.;
    view.retained_unit_health = true;
    view.add(
        symbol,
        [
            position[0] / scale,
            (position[1] - 20. * camera.zoom) / scale,
        ],
        scale,
        |n| {
            !(u.attack == 0 && n.parents.iter().any(|p| p == "attack_ext")
                || ability.is_none() && n.parents.iter().any(|p| p == "ability_ext"))
        },
        |n| match n.id.as_str() {
            "health_txt" => Some(u.hp.to_string()),
            "attack_txt" => Some(u.attack.to_string()),
            "caption_txt" => Some(scene.item_name(u.id, "").into()),
            "ability_txt" => Some(crate::interface::lang(scene, ability?.1, "").into()),
            _ => None,
        },
        None,
    );
    for d in &mut view.draws[start..] {
        d.node.parents.push("source_unit_health".into());
        if d.node.parents.iter().any(|p| p == "attack_ext") {
            d.node.rect[0] += (attack_at[0] - attack[0]) * scale;
            d.node.rect[1] += (attack_at[1] - attack[1]) * scale;
        }
        if d.node.parents.iter().any(|p| p == "ability_ext") {
            d.node.rect[1] += (ability_y - ability_rect[1]) * scale;
        }
        if d.node.id == "progressbar" {
            d.progress = if u.hp_max > 0 {
                (hp / u.hp_max as f64).clamp(0., 1.)
            } else {
                0.
            };
        }
        if d.node.id == "icon_ability" {
            if let Some((name, _)) = ability {
                if let Some(icon) = c
                    .symbols
                    .get(&format!("unit_ability_{name}"))
                    .and_then(|s| s.nodes.first())
                {
                    let center = [
                        d.node.rect[0] + d.node.rect[2] / 2.,
                        d.node.rect[1] + d.node.rect[3] / 2.,
                    ];
                    d.node.texture = icon.texture.clone();
                    d.node.rect = [
                        center[0] - icon.rect[2] * scale / 2.,
                        center[1] - icon.rect[3] * scale / 2.,
                        icon.rect[2] * scale,
                        icon.rect[3] * scale,
                    ];
                }
            }
        }
    }
    Some(())
}
pub fn building(
    view: &mut View,
    scene: &Scene,
    camera: &Camera,
    position: [f64; 2],
    u: &crate::battle::Unit,
    hp: f64,
) -> Option<()> {
    let c = scene.interface.as_ref()?;
    let key = if u.attack == 0 {
        "battle_building_info_building"
    } else {
        "battle_building_info_unit"
    };
    let symbol = c.symbols.get(key)?;
    let scale = scale(camera);
    let margin = crate::world_markers::margin(u.id);
    let start = view.draws.len();
    view.retained_unit_health = true;
    view.add(
        symbol,
        [
            (position[0] + margin[0] * scale) / scale,
            (position[1] + margin[1] * camera.zoom) / scale,
        ],
        scale,
        |_| true,
        |n| match n.id.as_str() {
            "caption_txt" => Some(scene.item_name(u.id, "").into()),
            "health_txt" => Some(u.hp.to_string()),
            "attack_txt" => Some(u.attack.to_string()),
            "attack_caption_txt" => {
                Some(crate::interface::lang(scene, "#tooltip_attack", "").into())
            }
            _ => None,
        },
        None,
    );
    for d in &mut view.draws[start..] {
        d.node.parents.push("source_building_health".into());
        if d.node.id == "progressbar" {
            d.progress = if u.hp_max > 0 {
                (hp / u.hp_max as f64).clamp(0., 1.)
            } else {
                0.
            };
        }
    }
    Some(())
}

/// Base selection uses the same original Unit/Building tooltip factories as a
/// mission. Read upgraded source statistics without constructing a battle/save.
pub fn base(
    view: &mut View,
    scene: &Scene,
    state: &crate::production::State,
    camera: &Camera,
    uid: &str,
) -> Option<()> {
    let e = state.entities.get(uid)?;
    let id = e["type"].as_u64()? as u32;
    if state.gathering(uid) || e["isDeleted"] == true || e["isInBattle"] == true {
        return None;
    }
    let object = scene.world.as_ref()?.catalog.get(&id)?;
    let x = e["x"].as_f64()? - 16.;
    let y = e["y"].as_f64()? - 16.;
    let (w, h) = if matches!(
        e["direction"].as_u64().unwrap_or(0) as u32,
        crate::world::SW | crate::world::NE
    ) {
        (object.height, object.width)
    } else {
        (object.width, object.height)
    };
    let (a, b) = (x - (w / 2.).trunc(), y - (h / 2.).trunc());
    view.base_selection = Some(
        [[a, b], [a + w, b], [a + w, b + h], [a, b + h]]
            .map(|p| camera.screen(scene.projection.project(p[0], p[1]))),
    );
    let d = scene.combat.as_ref()?.defs.get(&id)?;
    let unit = d.group == "units";
    if !unit
        && !(d.group == "decor" && d.health > 0)
        && (d.group != "buildings"
            || e["state"].as_i64() != Some(3)
            || e["productionType"].as_i64().unwrap_or(0) != 0)
    {
        return None;
    }
    let hp = state.upgraded_stat(scene, id, "health", d.health as i64) as i32;
    let attack = state.upgraded_stat(scene, id, "attack", d.attack as i64) as i32;
    let x = e["x"].as_f64()? - 16.;
    let y = e["y"].as_f64()? - 16.;
    let u = crate::battle::Unit {
        uid: uid.into(),
        id,
        kind: if unit {
            "unit"
        } else if d.group == "decor" {
            "decor"
        } else {
            "building"
        }
        .into(),
        x,
        y,
        direction: crate::entity_state::direction_name(e["direction"].as_i64()? as i32).into(),
        fraction: "player".into(),
        w: d.width,
        h: d.height,
        hp,
        hp_max: hp,
        attack,
        distance: state.upgraded_stat(scene, id, "distance", d.distance as i64) as i32,
        speed: state.upgraded_stat(scene, id, "speed", d.speed as i64) as i32,
        ability: d.ability,
        heavy: d.heavy,
        enemy: false,
        own: true,
        mode: 0,
        zone: [0; 4],
        target: None,
        appropriate: true,
        damaged: false,
        profile: None,
        pending: false,
        by_trigger: false,
    };
    // SceneMain.showSelectionBackground uses integer realWidth/realHeight;
    // the anchor denotes the top vertex of a 1x1 tile, not its centre.
    let (w, h) = if matches!(u.direction.as_str(), "se" | "nw") {
        (u.w as f64, u.h as f64)
    } else {
        (u.h as f64, u.w as f64)
    };
    let (a, b) = (x - (w / 2.).trunc(), y - (h / 2.).trunc());
    view.base_selection = Some(
        [[a, b], [a + w, b], [a + w, b + h], [a, b + h]]
            .map(|p| camera.screen(scene.projection.project(p[0], p[1]))),
    );
    let radius = if unit { u.range() } else { u.distance };
    if radius > 0 {
        let r = radius as f64;
        let n = 2. * r + 1.;
        view.battle_attack_area = [
            [x - r, y - r],
            [x - r + n, y - r],
            [x - r + n, y - r + n],
            [x - r, y - r + n],
            [x - r, y - r],
        ]
        .map(|p| camera.screen(scene.projection.project(p[0], p[1])))
        .to_vec();
    }
    full(
        view,
        scene,
        camera,
        camera.screen(scene.projection.project(x, y)),
        &u,
        hp as f64,
    )
}
