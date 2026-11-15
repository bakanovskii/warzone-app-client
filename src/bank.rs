//! Explicit offline bank catalog. Artwork/localized names are original; free
//! quantities are for the user's local game, never an emulated App Store sale.
use crate::{
    Scene,
    production::{State, adjust},
};
use std::collections::BTreeMap;
#[derive(Clone, Copy)]
pub struct Offer {
    pub id: u32,
    pub cash: i64,
}
pub const OFFERS: [Offer; 5] = [
    Offer { id: 1, cash: 10 },
    Offer { id: 2, cash: 50 },
    Offer { id: 3, cash: 100 },
    Offer { id: 4, cash: 500 },
    Offer { id: 5, cash: 1000 },
];
pub fn offer(id: u32) -> Option<&'static Offer> {
    OFFERS.iter().find(|o| o.id == id)
}
pub fn available(scene: &Scene) -> bool {
    matches!(
        scene.provenance["local_bank"]["format"].as_str(),
        Some("warzone-local-free-bank-v1" | "warzone-local-free-bank-v2")
    ) && scene.interface.as_ref().is_some_and(|c| {
        OFFERS
            .iter()
            .all(|o| c.symbols.contains_key(&format!("bank_cash_{}", o.id)))
    })
}
impl State {
    pub fn claim_bank_offer(
        &self,
        scene: &Scene,
        id: u32,
        wall: i64,
    ) -> Result<(Self, String), String> {
        let o = offer(id).ok_or("Нет такого предложения банка")?;
        if !available(scene) {
            return Err("Для банка нужен новый пакет ресурсов".into());
        }
        if self.resources["realMoney"] >= i32::MAX as i64 - o.cash {
            return Err("Достигнут максимальный баланс банкнот".into());
        }
        let now = self.clock(scene.economy.as_ref().ok_or("Missing clock")?, wall)?;
        if now < self.last_action_time {
            return Err("Часы переведены назад".into());
        }
        let mut next = self.clone();
        adjust(&mut next.resources, "realMoney", o.cash)?;
        let prefs = next.systems.as_mut().ok_or("Missing native preferences")?;
        let free = prefs
            .integer("realMoneyFree")
            .checked_add(o.cash)
            .filter(|n| *n < i32::MAX as i64)
            .ok_or("Free cash balance overflow")?;
        prefs.set("realMoneyFree".into(), free);
        // Freeze published receipt IDs1..5. New quantities use6..10, so old
        // grants remain valid without rewriting their history or wallet.
        let receipt = id + 5;
        next.building_receipt(
            now,
            &format!("local-bank:cash:{receipt}"),
            receipt,
            "bank_free",
            BTreeMap::from([("realMoney".into(), o.cash)]),
            vec![],
        )?;
        next.validate(scene)?;
        Ok((next, format!("Получено {} банкнот", o.cash)))
    }
    pub(crate) fn validate_bank(&self) -> Result<(), String> {
        for a in self.actions.iter().filter(|a| a.kind == "bank_free") {
            let cash = match a.recipe {
                1 => 100,
                2 => 1000,
                3 => 10000,
                4 => 100000,
                5 => 1000000,
                6..=10 => offer(a.recipe - 5).unwrap().cash,
                _ => return Err("Unknown local bank offer".into()),
            };
            if a.uid != format!("local-bank:cash:{}", a.recipe)
                || a.gains != BTreeMap::from([("realMoney".into(), cash)])
                || !a.entropy.is_empty()
                || a.shop.is_some()
                || a.source.is_some()
                || a.at_ms.is_some()
                || !a.quest_events_pending.is_empty()
            {
                return Err("Invalid local bank receipt".into());
            }
        }
        Ok(())
    }
}
