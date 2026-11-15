//! NoResourcesDialog.updateGUI: authored resource groups and original prices.
use crate::{
    Camera, Scene,
    interface::{self, Action, View},
    production::State,
};
pub fn build(v: &mut View, scene: &Scene, state: &State, camera: &Camera, id: u32) -> Option<()> {
    let q = if scene
        .construction
        .as_ref()
        .is_some_and(|c| c.buildings.contains_key(&id))
    {
        state.building_shortfall(scene, id)
    } else {
        state.shop_entity_shortfall(scene, id)
    }
    .ok()?;
    build_quote(v, scene, camera, &q)
}
pub fn build_quote(
    v: &mut View,
    scene: &Scene,
    camera: &Camera,
    q: &crate::construction::Shortfall,
) -> Option<()> {
    let root = scene
        .interface
        .as_ref()?
        .symbols
        .get("building_shortfall")?;
    let s = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / s - root.size[0]) / 2.,
        (camera.height as f64 / s - root.size[1]) / 2.,
    ];
    v.modal = true;
    v.hits.clear();
    v.blocks.clear();
    v.popup_shade(root.nodes.first()?, camera);
    let start = v.draws.len();
    let resource = |n: &interface::Node| -> Option<&str> {
        if n.parents.iter().any(|p| p == "itemGold") {
            Some("playMoney")
        } else if n.parents.iter().any(|p| p == "itemFuel") {
            Some("fuel")
        } else if n.parents.iter().any(|p| p == "itemRes") {
            Some("iron")
        } else {
            None
        }
    };
    v.add(
        root,
        o,
        s,
        |n| {
            // placeItems is the source UIView placeholder, not visible artwork.
            if n.id == "placeItems" || n.parents.iter().any(|p| p == "itemItem") {
                return false;
            }
            resource(n).is_none_or(|k| q.resources.contains_key(k))
        },
        |n| {
            Some(match n.id.as_str() {
                "caption_txt" => interface::lang(scene, "#buy_need_resources", "").into(),
                "ok_txt" => interface::lang(scene, "#buy_for", "").into(),
                "buyprice_txt" => q.cash.to_string(),
                "count_txt" => resource(n)
                    .and_then(|k| q.resources.get(k))
                    .map_or(String::new(), ToString::to_string),
                _ => String::new(),
            })
        },
        None,
    );
    // Original normal dialog places its resource row in placeItems' bounds.
    let keys: Vec<_> = [
        ("playMoney", "itemGold"),
        ("fuel", "itemFuel"),
        ("iron", "itemRes"),
    ]
    .into_iter()
    .filter(|(k, _)| q.resources.contains_key(*k))
    .collect();
    let materials: Vec<_> = q
        .resources
        .iter()
        .filter_map(|(k, n)| {
            k.strip_prefix("item:")
                .and_then(|v| v.parse::<u32>().ok())
                .map(|id| (id, *n))
        })
        .collect();
    // Resource groups and special materials share the same authored row.
    let total = keys.len() + materials.len();
    let area = root.frames.get("placeItems")?;
    let caption = root.nodes.iter().find(|n| n.id == "caption_txt")?;
    let caption_height = crate::interface_text::candara_height(
        caption,
        interface::lang(scene, "#buy_need_resources", ""),
    );
    let step = if total > 2 { 45. } else { 50. };
    let row_y = area[1] + (caption_height + 35.).trunc();
    for (i, (_, group)) in keys.iter().enumerate() {
        let authored = root.slots.get(*group)?;
        let center = area[0] + (area[2] * 0.5).trunc()
            - (step * total.saturating_sub(1) as f64 * 0.5).trunc()
            + i as f64 * step;
        for d in &mut v.draws[start..] {
            if d.node.parents.iter().any(|p| p == group) {
                d.node.rect[0] += (center - authored[0]) * s;
                d.node.rect[1] += (row_y - authored[1]) * s;
            }
        }
    }
    for (i, (id, count)) in materials.iter().enumerate() {
        let center = area[0] + (area[2] * 0.5).trunc()
            - (step * total.saturating_sub(1) as f64 * 0.5).trunc()
            + (keys.len() + i) as f64 * step;
        let authored = root.slots.get("itemItem")?;
        let image = root.frames.get("itemImage")?;
        // NoResourcesDialog centers the generated icon inside itemImage,
        // then places the whole itemItem template on the same resource row.
        let image_center = [
            center + image[0] + image[2] / 2. - authored[0],
            row_y + image[1] + image[3] / 2. - authored[1],
        ];
        if let Some(icon) = scene
            .interface
            .as_ref()?
            .symbols
            .get(&format!("speedup_item_{id}"))
        {
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
            let f = (40. / (bounds[2] - bounds[0]))
                .min(32. / (bounds[3] - bounds[1]))
                .min(1.);
            v.add(
                icon,
                [
                    (o[0] + image_center[0]) / f - (bounds[0] + bounds[2]) / 2.,
                    (o[1] + image_center[1]) / f - (bounds[1] + bounds[3]) / 2.,
                ],
                s * f,
                |_| true,
                |_| None,
                None,
            );
        }
        let start = v.draws.len();
        v.add(
            root,
            [o[0] + center - authored[0], o[1] + row_y - authored[1]],
            s,
            |n| n.id == "count_txt" && n.parents.iter().any(|p| p == "itemItem"),
            |_| Some(count.to_string()),
            None,
        );
        for d in &mut v.draws[start..] {
            d.node.parents.push("source_material_shortfall".into());
        }
    }
    v.button(root, "ok_btn", o, s, Action::Confirm);
    v.button(root, "close_btn", o, s, Action::Dismiss);
    Some(())
}
