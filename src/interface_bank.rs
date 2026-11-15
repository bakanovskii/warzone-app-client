//! ShopDialog original bank2 cards and original generated cash symbols.
use crate::{
    Camera, Scene, bank,
    interface::{self, Action, View},
    production::State,
};
pub fn build(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    origin: [f64; 2],
    scroll: f64,
) {
    let c = scene.interface.as_ref().unwrap();
    let root = &c.symbols["shop"];
    let card = &c.symbols["shop_item_bank2"];
    let s = crate::hud::scale(camera);
    let left = root.frames["scroll_side_left"];
    let right = root.frames["scroll_side_right"];
    let clip = [
        (origin[0] + left[0]) * s,
        (origin[1] + left[1]) * s,
        (right[0] + right[2] - left[0]) * s,
        left[3] * s,
    ];
    let stride = 130.;
    v.scroll_max = ((5. + stride * bank::OFFERS.len() as f64) * s - clip[2]).max(0.);
    for (i, offer) in bank::OFFERS.iter().enumerate() {
        let o = [
            origin[0] + left[0] + 5. + 62.5 + i as f64 * stride - scroll / s,
            origin[1] + left[1] + 5.,
        ];
        if (o[0] + 62.5) * s < clip[0] || (o[0] - 62.5) * s > clip[0] + clip[2] {
            continue;
        }
        let enabled = state.resources["realMoney"] < i32::MAX as i64 - offer.cash;
        let start = v.draws.len();
        v.add(
            card,
            o,
            s,
            |n| {
                !n.parents.iter().any(|p| {
                    [
                        "buy_btn_promo",
                        "symbDiscount1",
                        "play_money_txt",
                        "play_money_icon",
                        "bestseller_txt",
                    ]
                    .contains(&p.as_str())
                })
            },
            |n| {
                Some(match n.id.as_str() {
                    "caption_txt" => {
                        interface::lang(scene, &format!("#bank_cash_{}", offer.id), "Банкноты")
                            .into()
                    }
                    "button_txt" => if enabled {
                        "Бесплатно"
                    } else {
                        "Лимит"
                    }
                    .into(),
                    "real_money_txt" => offer.cash.to_string(),
                    _ => String::new(),
                })
            },
            Some(clip),
        );
        for d in &mut v.draws[start..] {
            if d.node.id == "caption_txt" {
                d.node.color = 0;
            }
            if !enabled && d.node.parents.iter().any(|p| p == "buy_btn") {
                d.grayscale = true;
            }
            d.node.parents.push(format!("bank_offer_{}", offer.id));
        }
        let icon = &c.symbols[&format!("bank_cash_{}", offer.id)];
        let rect = card.frames["entity_image"];
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
        let f = (rect[2] / (bounds[2] - bounds[0]))
            .min(rect[3] / (bounds[3] - bounds[1]))
            .min(1.);
        let start = v.draws.len();
        v.add(
            icon,
            [
                (o[0] + rect[0] + rect[2] / 2.) / f - (bounds[0] + bounds[2]) / 2.,
                (o[1] + rect[1] + rect[3] / 2.) / f - (bounds[1] + bounds[3]) / 2.,
            ],
            s * f,
            |_| true,
            |_| None,
            Some(clip),
        );
        for d in &mut v.draws[start..] {
            d.node.id = format!("bank_cash_art_{}", offer.id);
        }
        if enabled {
            let at = v.hits.len();
            v.button(card, "buy_btn", o, s, Action::BankClaim(offer.id));
            for h in &mut v.hits[at..] {
                let x = h.rect[0].max(clip[0]);
                let right = (h.rect[0] + h.rect[2]).min(clip[0] + clip[2]);
                h.rect[0] = x;
                h.rect[2] = (right - x).max(0.);
            }
            v.hits.retain(|h| h.rect[2] > 0.);
        }
    }
}
