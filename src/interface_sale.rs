//! SceneMain.deleteItemStart: uses MessageBoxDialog tag101 and #WANT_SELL.
use crate::{
    Camera, Scene,
    interface::{self, View},
    interface_resources::number_text,
    production::{State, number},
};

pub fn caption(scene: &Scene, state: &State, uid: &str) -> Result<String, String> {
    let price = state.sale_quote(scene, uid)?;
    let item = &state.entities[uid];
    let template = interface::lang(
        scene,
        "#WANT_SELL",
        "Ты действительно хочешь продать \"[1]\" за [2|# монету|# монеты|# монет]?",
    );
    // Resolve the numeric second parameter before inserting the source alias:
    // an alias is text, never another localization expression.
    let name = scene.item_name(number(item, "type") as u32, "");
    let template = template
        .replace("[1]", "\u{1f}sale_name\u{1f}")
        .replace("[2", "[1");
    Ok(number_text(&template, price).replace("\u{1f}sale_name\u{1f}", name))
}

pub fn build(
    view: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    uid: &str,
) -> Option<()> {
    scene.interface.as_ref()?.symbols.get("message")?;
    let text = caption(scene, state, uid).ok()?;
    crate::interface_windows::message(view, scene, camera, &text, true);
    view.sale_tint_uid = Some(uid.into());
    Some(())
}
