//! Original EnergyRefillDialog: one transactional purchase, live free timer.
use crate::{
    Camera, Scene,
    interface::{self, Action, Hit, View},
    production::State,
};
pub fn available(scene: &Scene) -> bool {
    scene
        .interface
        .as_ref()
        .is_some_and(|c| c.symbols.contains_key("energy_refill"))
}
pub fn hud(v: &mut View, scene: &Scene, camera: &Camera) {
    let Some(root) = scene
        .interface
        .as_ref()
        .and_then(|c| c.symbols.get("energy_refill"))
    else {
        return;
    };
    let s = crate::hud::scale(camera);
    // CHUDLayer.init0xca...: both currency buttons/tapzones call
    // onIconTouchBank: -> SceneMain.onShopDisplayBank -> itemId1 (bank).
    for (key, action) in [
        ("energy", Action::Energy),
        ("real_money", Action::Bank),
        ("play_money", Action::Bank),
    ] {
        for suffix in ["tapzone", "btn"] {
            if let Some(r) = root.slots.get(&format!("hud_{key}_{suffix}")) {
                v.hits.push(Hit {
                    action,
                    rect: r.map(|n| n * s),
                });
            }
        }
    }
}
pub fn price(scene: &Scene) -> i64 {
    scene
        .progression
        .as_ref()
        .and_then(|p| p.special_items.get(&4000))
        .and_then(|i| i.cost.get("realMoney"))
        .copied()
        .unwrap_or(1)
}
pub fn full_message(scene: &Scene) -> String {
    interface::lang(scene, "#recharge_full_energy", "У тебя полная энергия!").into()
}
pub fn build(v: &mut View, scene: &Scene, state: &State, camera: &Camera, now: i64) -> Option<()> {
    let root = scene.interface.as_ref()?.symbols.get("energy_refill")?;
    let scale = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / scale - root.size[0]) / 2.,
        (camera.height as f64 / scale - root.size[1]) / 2.,
    ];
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.popup_shade(root.nodes.first()?, camera);
    let energy = state.energy(scene, now);
    let seconds = state.energy_next_seconds(scene, now);
    let first = v.draws.len();
    v.add(
        root,
        o,
        scale,
        |_| true,
        |n| {
            Some(match n.id.as_str() {
                "refill_hint_txt" => interface::lang(
                    scene,
                    if energy < 1 {
                        "#refill_hint_txt"
                    } else {
                        "#refill_hint_txt2"
                    },
                    "",
                )
                .into(),
                "refill_text" => interface::lang(scene, "#refill_for", "").into(),
                "refill_value" => price(scene).to_string(),
                "wait_timer_txt" => format!(
                    "{} {}:{:02}",
                    interface::lang(scene, "#wait", ""),
                    seconds / 60,
                    seconds % 60
                ),
                _ => String::new(),
            })
        },
        None,
    );
    // EnergyRefillDialog.init0x17907e/0x17910c: Russian phone hint shifts up10pt.
    if state.language_id() == 1 {
        for d in &mut v.draws[first..] {
            if d.node.id == "refill_hint_txt" {
                d.node.rect[1] -= 10. * scale;
            }
        }
    }
    v.button(root, "refill_btn", o, scale, Action::EnergyBuy);
    v.button(root, "wait_btn", o, scale, Action::EnergyWait);
    v.button(root, "close_btn", o, scale, Action::EnergyWait);
    Some(())
}
