//! Source world expansion markers and BuyEarth/NoResources popups.
use crate::{
    Camera, Scene,
    interface::{Action, Hit, View},
    production::State,
    world::expansion_position,
};
fn text(scene: &Scene, key: &str, fallback: &str) -> String {
    super::interface::lang(scene, key, fallback).to_owned()
}
pub fn markers(view: &mut View, scene: &Scene, state: &State, camera: &Camera) {
    let Some(symbol) = scene
        .interface
        .as_ref()
        .and_then(|c| c.symbols.get("land_marker"))
    else {
        return;
    };
    if state.mission.is_some() && !state.mission_paused() {
        return;
    }
    let z = camera.zoom;
    for id in state.available_expansions(scene) {
        let Ok([x, y]) = expansion_position(id as i32) else {
            continue;
        };
        let a = camera.screen(
            scene
                .projection
                .project((6 * x + 3) as f64, (6 * y + 3) as f64),
        );
        let origin = [a[0] / z, a[1] / z];
        view.add(
            symbol,
            origin,
            z,
            |_| true,
            |n| (n.id == "expand_txt").then(|| text(scene, "#EXPAND", "Расширить").to_uppercase()),
            None,
        );
        view.hits.push(Hit {
            action: Action::LandOpen(id),
            rect: [a[0] - 67.5 * z, a[1] - 34.5 * z, 132.3 * z, 47. * z],
        });
    }
}
pub fn build(
    view: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    id: u32,
    shortfall: bool,
) {
    let Some(c) = scene.interface.as_ref() else {
        return;
    };
    let Some(symbol) = c
        .symbols
        .get(if shortfall { "land_shortfall" } else { "land" })
    else {
        return;
    };
    let Ok(q) = state.expansion_quote(scene, id) else {
        return;
    };
    let s = crate::hud::scale(camera);
    let origin = [
        (camera.width as f64 / s - symbol.size[0]) * 0.5,
        (camera.height as f64 / s - symbol.size[1]) * 0.5,
    ];
    view.hits.clear();
    view.blocks.clear();
    view.modal = true;
    if let Some(n) = symbol.nodes.first() {
        view.popup_shade(n, camera);
    }
    view.add(
        symbol,
        origin,
        s,
        |_| true,
        |n| match n.id.as_str() {
            "header_txt" => Some(text(scene, "#expansion", "Расширение базы")),
            "buy_for_txt" | "buy_for2_txt" | "ok_txt" => Some(text(scene, "#BUY_FOR", "Купить за")),
            "money_txt" => Some(q.cash.to_string()),
            "pricemoney_txt" => Some(q.coins.to_string()),
            "caption_txt" => Some(text(
                scene,
                "#buy_need_resources",
                "Для продолжения нужны ресурсы",
            )),
            "count_txt" => Some(q.missing.to_string()),
            "buyprice_txt" => Some(q.topup.to_string()),
            _ => None,
        },
        None,
    );
    view.button(symbol, "close_btn", origin, s, Action::Close);
    if shortfall {
        view.button(symbol, "ok_btn", origin, s, Action::LandTopup);
    } else {
        view.button(symbol, "buy_btn", origin, s, Action::LandCoins);
        view.button(symbol, "buyreal_btn", origin, s, Action::LandCash);
    }
}

/// Draw.drawEarthExpansionAreas0xea179: only outside edges of owned6x6 plots,
/// SHLine green0x209010, thickness3, alpha0.5. Shared edges remain invisible.
pub fn boundary(scene: &Scene, state: &State, camera: &Camera) -> Vec<[[f64; 2]; 2]> {
    let Some(map) = state
        .systems
        .as_ref()
        .and_then(|s| s.preferences.get("expansionsMap"))
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };
    let owned: std::collections::BTreeSet<_> = map
        .keys()
        .filter_map(|k| k.parse::<i32>().ok())
        .filter_map(|id| crate::world::expansion_position(id).ok())
        .collect();
    let mut edges = Vec::new();
    for &[x, y] in &owned {
        for (neighbor, a, b) in [
            ([x - 1, y], [6 * x, 6 * y], [6 * x, 6 * y + 6]),
            ([x + 1, y], [6 * x + 6, 6 * y], [6 * x + 6, 6 * y + 6]),
            ([x, y - 1], [6 * x, 6 * y], [6 * x + 6, 6 * y]),
            ([x, y + 1], [6 * x, 6 * y + 6], [6 * x + 6, 6 * y + 6]),
        ] {
            if !owned.contains(&neighbor) {
                edges.push([
                    camera.screen(scene.projection.project(a[0] as f64, a[1] as f64)),
                    camera.screen(scene.projection.project(b[0] as f64, b[1] as f64)),
                ]);
            }
        }
    }
    edges
}
