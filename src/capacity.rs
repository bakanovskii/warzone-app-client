//! SceneMain.onProductionTouch0x88949: reserved resources, then depot/tent prompt.
use crate::{
    Scene, interface,
    production::{Command, State},
};
#[derive(Debug)]
pub struct Prompt {
    pub building: u32,
    pub offer: bool,
    pub text: String,
}
impl State {
    pub fn collection_capacity_prompt(&self, scene: &Scene, uid: &str, now: i64) -> Option<Prompt> {
        if self.gathering(uid) || !crate::world_markers::is_ready(scene, self, uid, now) {
            return None;
        }
        let mut target = None;
        if let Some(Command::Collect { recipe, .. }) = self.workshop_command(scene, uid, 0) {
            let c = scene.workshop.as_ref()?;
            let r = c.recipes.get(&recipe)?;
            if r.unit() {
                let (used, max) = self.population(scene);
                let need = self.upgraded_stat(
                    scene,
                    r.output,
                    "population",
                    *c.population.get(&r.output)?,
                );
                if need > 0 && used + need > max {
                    target = Some(1003)
                }
            }
        }
        if let Some(Command::Collect { recipe, .. }) = self.command(scene.economy.as_ref()?, uid, 0)
        {
            let c = scene.economy.as_ref()?;
            let r = c.recipes.get(&recipe)?;
            let reserved = self.reserved_gather_resources(scene).ok()?;
            if ["fuel", "iron"].iter().any(|k| {
                r.outputs.get(*k).is_some_and(|v| {
                    v.count > 0
                        && self.resources[*k] + reserved.get(*k).copied().unwrap_or(0) + v.count
                            > self.capacity(c, k)
                })
            }) {
                target = Some(1016)
            }
        }
        let id = target?;
        let b = scene.construction.as_ref()?.buildings.get(&id)?;
        // Original isTopQuantityBuild checks the absolute level35 maximum,
        // not today's wallet; the shop handles level/access restrictions.
        let offer = self.building_count(id) < b.max_count(35);
        let key = match (id, offer) {
            (1003, true) => "#you_need_supply_depot",
            (1003, false) => "#no_more_barracks",
            (_, true) => "#need_depot",
            _ => "#no_more_supplies",
        };
        Some(Prompt {
            building: id,
            offer,
            text: interface::lang(scene, key, "").into(),
        })
    }
}
