//! LevelupDialog.updateGUI: presentation of awards already committed by Levels.
use crate::{
    Camera, Scene,
    interface::{self, Action, Draw, View},
    production::State,
};

pub fn build(v: &mut View, scene: &Scene, camera: &Camera, level: i64, scroll: f64) -> Option<()> {
    build_animated(v, scene, camera, level, scroll, 0.)
}
pub fn build_animated(
    v: &mut View,
    scene: &Scene,
    camera: &Camera,
    level: i64,
    scroll: f64,
    seconds: f64,
) -> Option<()> {
    let c = scene.interface.as_ref()?;
    let levels = &scene.progression.as_ref()?.levels;
    let current = levels.iter().find(|l| l.id == level)?;
    // Levels.getNewElementsOnLevel includes only bShopUse items with nNeedLevel.
    let mut items: Vec<_> = scene
        .shop
        .as_ref()?
        .items
        .values()
        .filter(|i| i.min_level == level)
        .collect();
    items.sort_by_key(|i| (i.order, i.id));
    let root = c.symbols.get(if items.is_empty() {
        "levelup_short"
    } else {
        "levelup_full"
    })?;
    let s = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / s - root.size[0]) / 2.,
        (camera.height as f64 / s - root.size[1]) / 2. - 25.,
    ];
    v.draws.clear();
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.popup_shade(root.nodes.first()?, camera);
    let cash = current
        .awards
        .iter()
        .filter(|a| a.kind == "realMoney")
        .filter_map(|a| a.count.parse::<i64>().ok())
        .sum::<i64>();
    let energy = current.max_energy
        - levels
            .iter()
            .filter(|l| l.id < level)
            .max_by_key(|l| l.id)
            .map_or(0, |l| l.max_energy);
    v.add(
        root,
        o,
        s,
        |_| true,
        |n| {
            Some(match n.id.as_str() {
                "level_txt" => level.to_string(),
                "level_up_txt" => {
                    interface::lang(scene, "#LEVEL_UP_HEADER", "Получен [1] уровень!")
                        .replace("[1]", &level.to_string())
                }
                "reward_txt" => {
                    interface::lang(scene, "#LEVEL_UP_BONUS_HEADER", "Твоя награда:").into()
                }
                "available_txt" => {
                    interface::lang(scene, "#LEVEL_UP_NEW_GOODIES_HEADER", "Открыт доступ к:")
                        .into()
                }
                "energy_max_txt" => format!("{energy} MAX"),
                "count_txt" => cash.to_string(),
                _ => String::new(),
            })
        },
        None,
    );
    // initWithLevel raises the content25pt and adds a white UIScrollView
    // inside its authored gray scroll placeholder.
    for d in &mut v.draws {
        if d.node.id == "scroll" {
            d.node.kind = "solid".into();
            d.node.texture.clear();
            d.node.text.clear();
            d.node.color = 0xffffff;
        }
        // CABasicAnimation transform.rotation.z,2pi over15s, linear/repeating.
        if d.node.id == "glow" {
            d.node.rotation = seconds.rem_euclid(15.) * 24.;
        }
    }
    if !items.is_empty() {
        let template = c.symbols.get("levelup_item")?;
        let area = root.frames.get("scroll")?;
        let clip = [
            (o[0] + area[0]) * s,
            (o[1] + area[1]) * s,
            area[2] * s,
            area[3] * s,
        ];
        v.scroll_max = (items.len() as f64 * template.size[0] - area[2]).max(0.) * s;
        let offset = scroll / s;
        let start = if items.len() == 1 {
            (area[2] - template.size[0]) / 2.
        } else {
            0.
        };
        for (index, item) in items.iter().enumerate() {
            let p = [
                o[0] + area[0] + start + index as f64 * template.size[0] - offset,
                o[1] + area[1] + (area[3] - template.size[1]) / 2.,
            ];
            v.add(
                template,
                p,
                s,
                |_| true,
                |n| {
                    Some(if n.id == "nameItem_txt" {
                        item.name.clone()
                    } else {
                        String::new()
                    })
                },
                Some(clip),
            );
            if let Some(info) = scene.textures.get(&item.thumbnail) {
                let frame = template.frames.get("thumbImage")?;
                let mut n = root.nodes.first()?.clone();
                // UIImage natural point size is half its @2x PNG dimensions.
                let w = info.width as f64 / 2.;
                let h = info.height as f64 / 2.;
                n.id = format!("levelup_thumb_{}", item.id);
                n.kind = "image".into();
                n.texture = item.thumbnail.clone();
                n.text.clear();
                n.parents.clear();
                n.rect = [
                    (p[0] + frame[0] + (frame[2] - w) / 2.) * s,
                    (p[1] + frame[1] + (frame[3] - h) / 2.) * s,
                    w * s,
                    h * s,
                ];
                v.draws.push(Draw {
                    node: n,
                    clip: Some(clip),
                    progress: 1.,
                    opacity: 255,
                    grayscale: false,
                });
            }
        }
    }
    v.button(root, "ok_btn", o, s, Action::Close);
    v.button(root, "close_btn", o, s, Action::Close);
    Some(())
}

pub fn level(state: &State) -> i64 {
    state.resources.get("level").copied().unwrap_or(1)
}
