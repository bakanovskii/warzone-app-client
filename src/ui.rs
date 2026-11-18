//! Shared desktop overlays. All mutations remain in the native state model.
use crate::render::{PanelView, modal_layout};
use sdl2::{event::Event, keyboard::Keycode, mouse::MouseButton, rect::Point};
use warzone_app_client::{
    Camera, Scene,
    production::{State, resource_name},
    systems::Exercise,
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Achievements,
    Leagues,
    LeaguesHelp,
    Energy,
    Land,
    Quests,
    Inventory,
    Shop,
    Construction,
    Sell,
    Missions,
    Pvp,
    Settings,
    Production,
    ResourceProduction,
    ResourceSpeedup,
    Research,
    Upgrades,
}

struct Guide {
    started_revision: u64,
    menu_only: bool,
    entry: Option<warzone_app_client::quest_navigation::Entry>,
    aligned_stage: String,
    shop_category_done: bool,
    stage: String,
    shown_game: i64,
    outside_taps: u32,
    outside_press: bool,
    pointer_start: Option<[i32; 2]>,
    motion: Option<warzone_app_client::quest_navigation::CameraMotion>,
    since: f64,
    quest: u32,
    index: usize,
    route: warzone_app_client::quest_navigation::Route,
    uid: Option<String>,
}
#[derive(Default)]
pub struct Ui {
    guide: Option<Guide>,
    land_id: Option<u32>,
    land_shortfall: bool,
    pub kind: Option<Kind>,
    pub index: usize,
    shop_tab: u32,
    league_index: usize,
    leagues_from_profile: bool,
    pressed: Option<u8>,
    pub sell_uid: Option<String>,
    pub selection: Option<String>,
    pub editing: bool,
    pub preview_drag_geometry: Option<([f64; 2], [f64; 2])>,
    button_taps: u64,
    quest_completion_sounds: u64,
    observed_level: Option<i64>,
    pending_level: Option<i64>,
    level_up: Option<i64>,
    observed_ready: Option<std::collections::BTreeSet<u32>>,
    pending_completions: std::collections::BTreeSet<u32>,
    pub placement_only: bool,
    training_intro: Option<(warzone_app_client::production::ProductionSource, u32)>,
    training_intro_seen: bool,
    native_press: Option<warzone_app_client::interface::Action>,
    native_pointer: [f64; 2],
    scroll: f64,
    scroll_maximum: f64,
    scroll_seconds: f64,
    scroll_velocity: f64,
    scroll_page_target: Option<f64>,
    scroll_sample: Option<(std::time::Instant, f64)>,
    scroll_kind: Option<Kind>,
    scroll_drag: Option<(i32, f64)>,
    lang_popup: bool,
    pub notice: Option<String>,
    capacity_target: Option<u32>,
    build_shortfall: Option<u32>,
    pub production_shortfall: Option<warzone_app_client::production::Command>,
    recipe_access: Option<(String, u32)>,
    pub now: i64,
    pub animation_seconds: f64,
    marker_motion: warzone_app_client::world_markers::Playback,
    clearing_motion: warzone_app_client::obstacles::Playback,
    reward_motion: warzone_app_client::reward_effects::Playback,
    story_motion: warzone_app_client::interface_story::Playback,
    animate_bottom: bool,
    bottom_motion: std::cell::RefCell<warzone_app_client::interface_motion::BottomPlayback>,
    research_detail: bool,
    upgrade_unit: Option<u32>,
    upgrade_detail: Option<u32>,
    upgrade_info: bool,
    material_info: Option<u32>,
    viewport_scale: f64,
    mission_briefing: bool,
    mission_offset: Option<[f64; 2]>,
    mission_drag: Option<(i32, i32, [f64; 2])>,
}
pub enum EventResult {
    BuildingTopup,
    ProductionTopup(warzone_app_client::production::Command),
    ClaimAchievement(u32, usize),
    QuestStoryOpen(u32),
    QuestStoryNext,
    BonusPickup(u32),
    ClearObstacle(String),
    QuestGuide(u32, usize),
    BuyLand(u32, warzone_app_client::expansions::Payment),
    Pass,
    Consumed,
    Claim(u32),
    Buy(u32),
    PlanBuilding(u32),
    Sell(String),
    SaleCancelled,
    OpenMission(String),
    ToggleAudio(String),
    SetLanguage(u32),
    Key(Keycode),
    Notice(String),
    Production(usize),
    UnlockShop(u32),
    BuyMaterial(u32),
    BankClaim(u32),
    BuyResearch(String, u32),
    ResearchUnlock(String, u32),
    RecipeUnlock(String, u32),
    ResourceSpeedup(String),
    ObjectPrimary(String),
    ObjectCollect(warzone_app_client::production::Command),
    ObjectFinish(String),
    ObjectRepair(String),
    ObjectResearchCollect(warzone_app_client::production::Command),
    ObjectWorkshopCollect(warzone_app_client::production::Command),
    ObjectTraining(warzone_app_client::production::ProductionSource, u32),
    UpgradeApply(warzone_app_client::production::Command),
    BuyUpgrade(String, u32),
    AcademyResearch(String, u32),
}
impl Ui {
    pub fn begin_sale(&mut self, uid: String) {
        self.selection = Some(uid.clone());
        self.sell_uid = Some(uid);
        self.kind = Some(Kind::Sell);
        self.guide = None;
        self.notice = None;
        self.recipe_access = None;
        self.native_press = None;
        self.scroll_drag = None;
        self.pressed = None;
    }
    fn cancel_sale(&mut self) -> EventResult {
        self.kind = None;
        self.sell_uid = None;
        self.selection = None;
        self.guide = None;
        self.native_press = None;
        EventResult::SaleCancelled
    }
    fn confirm_sale(&mut self, scene: &Scene, state: &State) -> EventResult {
        let Some(uid) = self.sell_uid.as_deref() else {
            return EventResult::Consumed;
        };
        if let Err(text) = state.sale_quote(scene, uid) {
            return EventResult::Notice(text);
        }
        let uid = self.sell_uid.take().unwrap();
        self.kind = None;
        self.selection = None;
        self.guide = None;
        self.native_press = None;
        EventResult::Sell(uid)
    }
    fn next_mission(scene: &Scene, state: &State) -> usize {
        if state.mission_paused() {
            if let Some(index) = state.mission.as_ref().and_then(|m| {
                scene.combat.as_ref().and_then(|c| {
                    c.campaign_names()
                        .position(|name| name == &m.battle.mission)
                })
            }) {
                return index;
            }
        }
        scene
            .combat
            .as_ref()
            .and_then(|c| {
                c.campaign()
                    .enumerate()
                    .find(|(_, m)| {
                        !state
                            .systems
                            .as_ref()
                            .is_some_and(|p| p.flag(&format!("Mission-{}", m.mission_id)))
                            && state.mission_access(scene, &m.mission).is_ok()
                    })
                    .map(|(index, _)| index)
            })
            .unwrap_or(0)
    }
    fn close_upgrades(&mut self) {
        if self.upgrade_detail.take().is_some() {
            self.upgrade_info = false;
        } else if self.upgrade_unit.take().is_some() {
            self.scroll = 0.;
        } else {
            self.kind = None;
        }
    }
    pub fn upgrade_bought(&mut self) {
        self.upgrade_detail = None;
        self.upgrade_info = false;
    }
    pub fn academy_research(&mut self, scene: &Scene, uid: String, unit: u32) {
        self.selection = Some(uid);
        self.kind = Some(Kind::Research);
        self.research_detail = true;
        self.index = warzone_app_client::interface_windows::RESEARCH_ORDER
            .iter()
            .position(|id| {
                scene
                    .research
                    .as_ref()
                    .and_then(|c| c.items.get(id))
                    .is_some_and(|r| r.unit == unit)
            })
            .unwrap_or(0);
    }
    /// SceneMain.onTouch: a busy building opens immediately; an idle building
    /// first selects, then opens on the next tap. Opening never sends a command.
    pub fn open_selected_window(
        &mut self,
        scene: &Scene,
        state: &State,
        uid: &str,
        busy_only: bool,
    ) -> bool {
        if self.kind.is_some()
            || self.editing
            || self.notice.is_some()
            || state.gathering(uid)
            || state.repair_pending(scene, uid)
        {
            return false;
        }
        let Some(e) = state.entities.get(uid) else {
            return false;
        };
        if !scene
            .world
            .as_ref()
            .and_then(|c| c.catalog.get(&(e["type"].as_u64()? as u32)))
            .is_some_and(|e| e.kind == "buildings")
        {
            return false;
        }
        let active = scene
            .economy
            .as_ref()
            .and_then(|c| self.now.checked_sub(c.time_shift))
            .is_some_and(|wall| state.production_speedup_quote(scene, uid, wall).is_ok());
        let kind = if active {
            Kind::ResourceSpeedup
        } else {
            if busy_only
                || e["productionType"].as_i64() != Some(0)
                || state.construction_pending(scene, uid)
            {
                return false;
            }
            if e["type"] == 1019 && scene.upgrades.is_some() {
                Kind::Upgrades
            } else if e["type"] == 1020 && state.research_command(scene, uid, 0).is_some() {
                Kind::Research
            } else if state.workshop_choice(scene, uid, 0).is_some() {
                Kind::Production
            } else if scene
                .economy
                .as_ref()
                .is_some_and(|c| state.command(c, uid, 0).is_some())
            {
                Kind::ResourceProduction
            } else {
                return false;
            }
        };
        self.selection = Some(uid.to_owned());
        self.kind = Some(kind);
        self.scroll = 0.;
        if kind == Kind::Upgrades {
            self.upgrade_unit = None;
            self.upgrade_detail = None;
            self.scroll = 30.
                * if self.viewport_scale > 0. {
                    self.viewport_scale
                } else {
                    2.
                };
        }
        self.index = 0;
        self.research_detail = false;
        true
    }
    pub fn update_time_ms(&mut self, scene: &Scene, state: &State, now_ms: i64) {
        let elapsed = (self.animation_seconds - self.scroll_seconds).max(0.);
        self.scroll_seconds = self.animation_seconds;
        if self.scroll_kind != self.kind {
            self.scroll_velocity = 0.;
            self.scroll_page_target = None;
            self.scroll_sample = None;
            self.scroll_kind = self.kind;
        }
        // ShopDialog disables UIScrollView scrolling during its item guide.
        // Its explicit centered contentOffset may exceed the normal last page.
        let guided_shop = self.guide.as_ref().is_some_and(|g| {
            g.shop_category_done
                && g.uid.is_none()
                && matches!(
                    g.route,
                    warzone_app_client::quest_navigation::Route::Building { .. }
                )
        }) && matches!(self.kind, Some(Kind::Construction | Kind::Shop));
        if self.kind == Some(Kind::LeaguesHelp) {
            if self.scroll_drag.is_none() {
                if let Some(target) = self.scroll_page_target {
                    self.scroll =
                        target + (self.scroll - target) * (-16. * elapsed.clamp(0., 0.1)).exp();
                    if (self.scroll - target).abs() < 0.1 * self.viewport_scale.max(1.) {
                        self.scroll = target;
                        self.scroll_page_target = None;
                    }
                }
            }
        } else if self.scroll_drag.is_none() && !guided_shop {
            self.scroll = warzone_app_client::scroll_motion::coast(
                self.scroll,
                &mut self.scroll_velocity,
                self.scroll_maximum,
                elapsed,
            );
        }
        self.story_motion
            .observe(scene, state, true, self.animation_seconds);
        let now = now_ms / 1000;
        self.reward_motion
            .update(scene, state, self.animation_seconds);
        let level = warzone_app_client::interface_levelup::level(state);
        if self
            .observed_level
            .replace(level)
            .is_some_and(|old| level > old)
        {
            self.pending_level = Some(level);
        }
        if self.level_up.is_none()
            && self.pending_level.is_some()
            && state.mission.is_none()
            && !self.editing
            && self.guide.is_none()
            && state.quest_story().ok().flatten().is_none()
        {
            self.level_up = self.pending_level.take();
            self.scroll = 0.;
            self.native_press = None;
        }

        let ready: std::collections::BTreeSet<_> = state
            .active_quests(scene)
            .into_iter()
            .filter(|q| state.quest_ready(scene, q))
            .map(|q| q.id)
            .collect();
        if let Some(before) = self.observed_ready.replace(ready.clone()) {
            self.pending_completions
                .extend(ready.difference(&before).copied());
        }
        self.pending_completions.retain(|q| ready.contains(q));
        if self.kind == Some(Kind::Quests) {
            if let Some(q) = state.active_quests(scene).get(self.index) {
                if self.pending_completions.remove(&q.id) {
                    self.quest_completion_sounds += 1;
                }
            }
        }
        if state.mission.is_none()
            && state.quest_story().ok().flatten().is_none()
            && !self.editing
            && self.guide.is_none()
            && self.notice.is_none()
            && self.kind.is_none()
            && self.level_up.is_none()
        {
            if let Some(q) = self.pending_completions.pop_first() {
                self.index = state
                    .active_quests(scene)
                    .iter()
                    .position(|v| v.id == q)
                    .unwrap_or(0);
                self.kind = Some(Kind::Quests);
                self.quest_completion_sounds += 1;
            }
        }
        if let Some(p) = state.quest_story().ok().flatten() {
            self.kind = Some(Kind::Quests);
            self.index = state
                .active_quests(scene)
                .iter()
                .position(|q| q.id == p.quest)
                .unwrap_or(0);
        }
        self.now = now;
        if self.kind == Some(Kind::Energy) && state.energy(scene, now) >= state.max_energy(scene) {
            self.kind = None;
            self.native_press = None;
        }
        // A mini-tutorial is one source command path, not the whole objective.
        // Retain the opening revision/UID so an older receipt or another
        // producer cannot finish this particular guidance session.
        if self.guide.as_ref().is_some_and(|g| {
            state.actions.iter().any(|a| {
                a.sequence > g.started_revision
                    && warzone_app_client::quest_navigation::finishes(&g.route, g.uid.as_deref(), a)
            })
        }) {
            self.guide = None;
        }
        if let Some(g) = &mut self.guide {
            let stage = format!(
                "{:?}:{:?}:{}:{:?}:{:?}:{}:{}",
                self.kind,
                self.selection,
                self.editing,
                self.upgrade_unit,
                self.upgrade_detail,
                self.research_detail,
                self.mission_briefing
            );
            if g.stage != stage {
                g.stage = stage;
                g.since = self.animation_seconds;
                g.shown_game = self.now;
                g.outside_taps = 0;
                g.outside_press = false;
                g.pointer_start = None;
            }
            if let Some(route) =
                warzone_app_client::quest_navigation::route(scene, state, g.quest, g.index)
            {
                // Source builds its PrepareTo* step list once at entry. A
                // missing producer becomes a full construction guide; buying
                // its unfinished archive must not silently switch that same
                // guide back to production/research before construction ends.
                let route = if matches!(
                    g.route,
                    warzone_app_client::quest_navigation::Route::Building { .. }
                ) && !matches!(
                    route,
                    warzone_app_client::quest_navigation::Route::Building { .. }
                ) {
                    g.route.clone()
                } else {
                    route
                };
                // Keep the chosen expansion when the source exercise means any
                // area; all producer/object targets are live, including after buy.
                if !matches!(
                    route,
                    warzone_app_client::quest_navigation::Route::Expansion(0)
                        | warzone_app_client::quest_navigation::Route::Speedup
                ) {
                    let (next_route, next_uid) =
                        warzone_app_client::quest_navigation::target(scene, state, route);
                    if next_route != g.route || next_uid != g.uid {
                        g.since = self.animation_seconds;
                    }
                    (g.route, g.uid) = (next_route, next_uid);
                }
            } else {
                self.guide = None;
            }
        }
        self.marker_motion
            .update_ms(scene, state, now_ms, self.animation_seconds);
        if self.kind == Some(Kind::ResourceSpeedup)
            && self.selection.as_deref().is_none_or(|uid| {
                scene.economy.as_ref().is_none_or(|c| {
                    now.checked_sub(c.time_shift).is_none_or(|wall| {
                        state.production_speedup_quote(scene, uid, wall).is_err()
                    })
                })
            })
        {
            self.kind = None;
        }
    }
    pub fn world_markers(
        &self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) -> warzone_app_client::interface::View {
        let mut view = warzone_app_client::world_markers::build_selected(
            scene,
            state,
            camera,
            self.now,
            Some(&self.marker_motion),
            self.animation_seconds,
            self.selection.as_deref(),
            true,
        );
        warzone_app_client::obstacles::effects(
            &mut view,
            scene,
            state,
            camera,
            self.now,
            self.animation_seconds,
            Some(&self.clearing_motion),
        );
        if self.kind.is_none() && !self.editing {
            if let Some(uid) = &self.selection {
                warzone_app_client::interface_unit_health::base(
                    &mut view, scene, state, camera, uid,
                );
            }
        }
        view.world_boundary = warzone_app_client::interface_land::boundary(scene, state, camera);
        self.reward_motion
            .draw(&mut view, scene, state, camera, self.animation_seconds);
        if !self.editing {
            warzone_app_client::interface_land::markers(&mut view, scene, state, camera);
        }
        view
    }
    pub fn initialize_level(&mut self, state: &State) {
        self.observed_level = Some(warzone_app_client::interface_levelup::level(state));
    }
    pub fn initialize_receipts(&mut self, revision: u64) {
        self.reward_motion.initialize(revision);
    }
    pub fn prepare_reward_camera(&mut self, scene: &Scene, camera: &Camera) {
        self.reward_motion.prepare_camera(scene, camera);
    }
    pub fn take_reward_sounds(&mut self) -> Vec<&'static str> {
        self.reward_motion.take_sounds()
    }
    pub fn story_input_ready(&self) -> bool {
        self.story_motion.input_ready(self.animation_seconds)
    }
    /// Real application enables kinematics; pure layout callers can inspect a
    /// settled source view without constructing a clock or mutating a profile.
    pub fn owns_native_pointer(&self) -> bool {
        self.native_press.is_some() || self.scroll_drag.is_some() || self.mission_drag.is_some()
    }
    pub fn take_button_taps(&mut self) -> u64 {
        std::mem::take(&mut self.button_taps)
    }
    pub fn take_quest_completion_sounds(&mut self) -> u64 {
        std::mem::take(&mut self.quest_completion_sounds)
    }
    pub fn begin_training_instruction(
        &mut self,
        state: &State,
        source: &warzone_app_client::production::ProductionSource,
        id: u32,
    ) -> bool {
        let first = state.systems.as_ref().is_some_and(|s| {
            ["FirstUnitPlacing", "FirstUnitPlacingDialog"]
                .into_iter()
                .all(|k| {
                    s.preferences
                        .get(k)
                        .is_none_or(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0))
                })
        });
        if !self.training_intro_seen
            && first
            && id <= 2020
            && !state.actions.iter().any(|a| a.kind == "unit_train")
        {
            self.training_intro = Some((source.clone(), id));
            self.training_intro_seen = true;
            self.kind = None;
            true
        } else {
            false
        }
    }
    pub fn enable_bottom_motion(&mut self) {
        self.animate_bottom = true;
    }
    pub fn cancel_pointer(&mut self) {
        self.native_press = None;
        self.pressed = None;
        self.scroll_drag = None;
    }
    pub fn bottom_input_ready(&self) -> bool {
        !self.animate_bottom || !self.bottom_motion.borrow().active(self.animation_seconds)
    }
    pub fn reward_hud_values(
        &self,
        scene: &Scene,
        state: &State,
        wall: i64,
    ) -> warzone_app_client::hud::Values {
        self.reward_motion.hud_values(scene, state, wall)
    }

    pub fn source_report(
        &self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) -> Option<serde_json::Value> {
        let v = self.original_view(scene, state, camera)?;
        let markers = self.world_markers(scene, state, camera);
        let mut report = serde_json::json!({"story_key":warzone_app_client::quest_story::current(scene,state).map(|p|p.0),"story_modal":state.quest_story().ok().flatten().is_some(),"screen":format!("{:?}",self.kind),"league_selected":if self.kind==Some(Kind::Leagues){Some(self.league_index)}else{None},"leagues_help_page":if self.kind==Some(Kind::LeaguesHelp){Some(warzone_app_client::interface_leagues::page(scene,camera,self.scroll)+1)}else{None},"draws":v.draws.len(),"hit_targets":v.hits.len(),"scroll":self.scroll,"scroll_max":v.scroll_max,"source_geometry":true,"quest_guide":self.guide.as_ref().map(|g|serde_json::json!({"quest":g.quest,"exercise":g.index,"route":format!("{:?}",g.route),"uid":g.uid,"scrolling":g.motion.is_some(),"outside_taps":g.outside_taps})),"guide_cancel_visible":v.hits.iter().any(|h|h.action==warzone_app_client::interface::Action::TutorialCancel),"guide_focus":v.draws.iter().filter(|d|d.node.id=="source_quest_focus").map(|d|serde_json::json!({"rect":d.node.rect,"kind":d.node.kind})).collect::<Vec<_>>(),"guide_arrows":v.draws.iter().filter(|d|d.node.id=="source_tutorial_arrow").count(),"world_marker_ids":markers.draws.iter().map(|d|&d.node.id).collect::<Vec<_>>(),"world_marker_texts":markers.draws.iter().filter(|d|d.node.kind=="text").map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text})).collect::<Vec<_>>(),"ready_marker_targets":markers.hits.len(),"notice":self.notice.is_some(),"recipe_access_prompt":self.recipe_access.is_some(),"campaign_briefing":self.mission_briefing,"campaign_offset":self.mission_offset,"research_detail":self.research_detail,"upgrade_unit":self.upgrade_unit,"upgrade_detail":self.upgrade_detail,"upgrade_info":self.upgrade_info,"speedup_bindings":v.draws.iter().filter(|d|["timer_txt","buyprice_txt","name_txt","value_txt"].contains(&d.node.id.as_str())).map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text})).collect::<Vec<_>>(),"contract_bindings":v.draws.iter().filter(|d|["name0_txt","time0_txt","res0_txt","produce0_txt","caption_txt"].contains(&d.node.id.as_str())).map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text})).collect::<Vec<_>>(),"purchase_bindings":v.draws.iter().filter(|d|["unlockprice_txt","unlock1_txt","item_price_txt","count_txt"].contains(&d.node.id.as_str())).map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text,"parents":d.node.parents})).collect::<Vec<_>>(),"title_bindings":v.draws.iter().filter(|d|["caption_txt","header_txt"].contains(&d.node.id.as_str())).map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text})).collect::<Vec<_>>()});
        report["bank_bindings"]=serde_json::json!(v.draws.iter().filter(|d|d.node.parents.iter().any(|p|p.starts_with("bank_offer_"))&&d.node.kind=="text").map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text,"rect":d.node.rect,"color":d.node.color,"parents":d.node.parents})).collect::<Vec<_>>());

        report["level_up"] = self.level_up.into();
        report["review104_bindings"]=serde_json::to_value(v.draws.iter().filter(|d|["gcBtn_txt","description_txt","researchtime_txt","startresearch_hint","energy_max_txt"].contains(&d.node.id.as_str())).map(|d|serde_json::json!({"id":d.node.id,"text":d.node.text,"rect":d.node.rect,"font_size":d.node.font_size})).collect::<Vec<_>>()).ok()?;
        report["levelup_texts"] = serde_json::to_value(
            v.draws
                .iter()
                .filter(|d| {
                    [
                        "level_txt",
                        "level_up_txt",
                        "energy_max_txt",
                        "nameItem_txt",
                    ]
                    .contains(&d.node.id.as_str())
                })
                .map(|d| (&d.node.id, &d.node.text))
                .collect::<Vec<_>>(),
        )
        .ok()?;
        Some(report)
    }
    pub fn original_view(
        &self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) -> Option<warzone_app_client::interface::View> {
        self.original_view_with_markers(scene, state, camera, None)
    }
    /// The frame renderer already needs these exact markers for drawing. Reuse
    /// their hit regions instead of building every tooltip a second time.
    pub fn original_view_with_markers(
        &self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
        markers: Option<&warzone_app_client::interface::View>,
    ) -> Option<warzone_app_client::interface::View> {
        use warzone_app_client::interface::Screen;
        let screen = match self.kind {
            Some(Kind::Inventory) => Some(Screen::Inventory),
            Some(Kind::Settings) => Some(Screen::Settings),
            _ => None,
        };
        let mut view = warzone_app_client::interface::build(
            scene,
            state,
            camera,
            screen,
            if self.placement_only {
                None
            } else {
                self.selection.as_deref()
            },
            self.editing,
            self.scroll,
            self.lang_popup,
        )?;
        if self.kind == Some(Kind::Achievements) {
            warzone_app_client::interface_achievements::build(
                &mut view,
                scene,
                state,
                camera,
                self.scroll,
            );
        }
        if self.animate_bottom {
            let scale = warzone_app_client::hud::scale(camera);
            let (key, travel) = if self.kind == Some(Kind::Achievements) {
                (
                    "achievements".to_string(),
                    (scene.interface.as_ref()?.symbols["achievements"].size[1] - 84.) * scale,
                )
            } else {
                match screen {
                    Some(Screen::Settings) => (
                        "settings".to_string(),
                        scene.interface.as_ref()?.symbols["settings"].size[1] * scale,
                    ),
                    Some(Screen::Inventory) => (
                        "inventory".to_string(),
                        scene.interface.as_ref()?.symbols["inventory"].size[1] * scale,
                    ),
                    None => (
                        if self.editing {
                            "edit".into()
                        } else {
                            self.selection
                                .as_ref()
                                .map_or_else(|| "main".into(), |uid| format!("item:{uid}"))
                        },
                        65. * scale,
                    ),
                }
            };
            self.bottom_motion.borrow_mut().apply(
                key,
                travel,
                camera,
                self.animation_seconds,
                &mut view,
            );
        }
        if self.kind.is_none() && !self.editing {
            if let Some(markers) = markers {
                view.hits.splice(0..0, markers.hits.iter().cloned());
            } else {
                view.hits
                    .splice(0..0, self.world_markers(scene, state, camera).hits);
            }
        }
        if self.kind == Some(Kind::Leagues) {
            warzone_app_client::interface_leagues::build(
                &mut view,
                scene,
                state,
                camera,
                self.league_index,
            );
        }
        if self.kind == Some(Kind::LeaguesHelp) {
            warzone_app_client::interface_leagues::help(
                &mut view,
                scene,
                state,
                camera,
                self.scroll,
            );
        }
        if self.kind == Some(Kind::Energy) {
            warzone_app_client::interface_energy::build(&mut view, scene, state, camera, self.now);
        }
        if self.kind == Some(Kind::Land) {
            if let Some(id) = self.land_id {
                warzone_app_client::interface_land::build(
                    &mut view,
                    scene,
                    state,
                    camera,
                    id,
                    self.land_shortfall,
                );
            }
        }
        if self.kind == Some(Kind::Missions) {
            warzone_app_client::interface_campaign::build(
                &mut view,
                scene,
                state,
                camera,
                self.index,
                self.mission_briefing,
                self.mission_offset
                    .unwrap_or_else(warzone_app_client::interface_campaign::initial_offset),
                self.animation_seconds,
            );
        }
        if self.kind == Some(Kind::Pvp) {
            warzone_app_client::interface_pvp::profile(&mut view, scene, state, camera);
        }
        if self.kind == Some(Kind::Upgrades) {
            warzone_app_client::interface_upgrades::build(
                &mut view,
                scene,
                state,
                camera,
                self.upgrade_unit,
                self.upgrade_detail,
                self.upgrade_info,
                self.scroll,
            );
        }
        if self.kind == Some(Kind::Production) {
            if let Some(uid) = self.selection.as_deref() {
                warzone_app_client::interface_windows::workshop(
                    &mut view,
                    scene,
                    state,
                    camera,
                    uid,
                    self.scroll,
                    self.now,
                );
            }
        }
        if self.kind == Some(Kind::Research) {
            if let Some(uid) = self.selection.as_deref() {
                warzone_app_client::interface_windows::research(
                    &mut view,
                    scene,
                    state,
                    camera,
                    uid,
                    if self.research_detail {
                        Some(self.index)
                    } else {
                        None
                    },
                    self.now,
                );
            }
        }
        if matches!(self.kind, Some(Kind::Construction | Kind::Shop)) && scene.shop.is_some() {
            warzone_app_client::interface_shop::build(
                &mut view,
                scene,
                state,
                camera,
                if (1..=7).contains(&self.shop_tab) {
                    self.shop_tab
                } else if self.kind == Some(Kind::Shop) {
                    7
                } else {
                    2
                },
                self.scroll,
            );
        }
        if self.kind == Some(Kind::Quests) && state.quest_story().ok().flatten().is_none() {
            warzone_app_client::interface_windows::quests(
                &mut view,
                scene,
                state,
                camera,
                self.index,
                self.scroll,
            );
        }
        if self.kind == Some(Kind::ResourceProduction) {
            if let Some(uid) = self.selection.as_deref() {
                warzone_app_client::interface_resources::contracts(
                    &mut view, scene, state, camera, uid,
                );
            }
        }
        if self.kind == Some(Kind::ResourceSpeedup) {
            if let Some(uid) = self.selection.as_deref() {
                warzone_app_client::interface_resources::active_speedup(
                    &mut view, scene, state, camera, uid, self.now,
                );
            }
        }
        if self.kind == Some(Kind::Sell) {
            if let Some(uid) = self.sell_uid.as_deref() {
                warzone_app_client::interface_sale::build(&mut view, scene, state, camera, uid);
            }
        }
        if let Some((_, id)) = &self.recipe_access {
            if let Some(price) = scene
                .economy
                .as_ref()
                .and_then(|c| c.recipes.get(id))
                .map(|r| r.unlock_price.unwrap_or(0))
                .or_else(|| {
                    scene
                        .workshop
                        .as_ref()
                        .and_then(|c| c.recipes.get(id))
                        .map(|r| r.unlock_price)
                })
            {
                let text = warzone_app_client::interface_resources::number_text(
                    scene
                        .progression
                        .as_ref()
                        .and_then(|c| c.language.get("#WANT_TO_UNLOCK_RECIPE"))
                        .map(String::as_str)
                        .unwrap_or("Открыть доступ за [1] банкнот?"),
                    price,
                );
                warzone_app_client::interface_windows::message(
                    &mut view, scene, camera, &text, true,
                );
            }
        }
        if let Some(cmd) = &self.production_shortfall {
            if let Ok(q) = state.production_shortfall(scene, cmd) {
                warzone_app_client::interface_shortfall::build_quote(&mut view, scene, camera, &q);
            }
        }
        if let Some(id) = self.build_shortfall {
            warzone_app_client::interface_shortfall::build(&mut view, scene, state, camera, id);
        }
        if let Some(text) = &self.notice {
            warzone_app_client::interface_windows::message(
                &mut view,
                scene,
                camera,
                text,
                self.capacity_target.is_some(),
            );
            if self.capacity_target == Some(1020) {
                for d in &mut view.draws {
                    if d.node.kind == "text" {
                        if d.node.parents.iter().any(|p| p == "ok_btn") {
                            d.node.text = warzone_app_client::interface::lang(
                                scene,
                                "#show_button",
                                "Показать",
                            )
                            .into();
                        } else if d.node.parents.iter().any(|p| p == "cancel_btn") {
                            d.node.text =
                                warzone_app_client::interface::lang(scene, "#cancel", "Отмена")
                                    .into();
                        }
                    }
                }
            }
        }
        if let Some(id) = self.material_info {
            warzone_app_client::interface_upgrades::material_info(&mut view, scene, camera, id);
        }
        if matches!(
            self.kind,
            Some(
                Kind::Construction
                    | Kind::Shop
                    | Kind::Production
                    | Kind::Research
                    | Kind::Upgrades
                    | Kind::Leagues
                    | Kind::LeaguesHelp
            )
        ) {
            if let Some(d) = view.draws.first_mut() {
                d.node.parents.push("source_pause_world".into());
            }
        }
        view.scrolling = self.scroll_drag.is_some()
            || self.scroll_page_target.is_some()
            || self.scroll_velocity != 0.
            || self.scroll < 0.
            || self.scroll > self.scroll_maximum;
        self.guide_overlay(&mut view, scene, state, camera);
        if self.kind.is_none()
            && !self.editing
            && self.notice.is_none()
            && state.quest_story().ok().flatten().is_none()
        {
            warzone_app_client::interface_windows::quest_notifications(
                &mut view,
                scene,
                state,
                camera,
                self.animation_seconds,
            );
        }
        if self.training_intro.is_some() {
            warzone_app_client::interface_story::add_message(
                &mut view,
                scene,
                state,
                camera,
                "#dialog_drag_trained_unit",
                scene
                    .progression
                    .as_ref()?
                    .language
                    .get("#dialog_drag_trained_unit")?,
            );
        }
        if state.quest_story().ok().flatten().is_some() {
            warzone_app_client::interface_story::add(&mut view, scene, state, camera);
            self.story_motion
                .apply(&mut view, camera, self.animation_seconds);
        }
        if let Some(level) = self.level_up {
            warzone_app_client::interface_levelup::build_animated(
                &mut view,
                scene,
                camera,
                level,
                self.scroll,
                self.animation_seconds,
            );
        }
        // UIButton2 swaps only its background on down, then restores it on
        // release/outside/scroll. Labels and currency glyphs remain unchanged.
        if let Some(action) = self.native_press {
            let [x, y] = self.native_pointer;
            if view.hit(x as i32, y as i32) == Some(action) {
                if let Some(hit) = view.hits.iter().rev().find(|h| {
                    h.action == action && warzone_app_client::interface::contains(h.rect, [x, y])
                }) {
                    let centre = [
                        hit.rect[0] + hit.rect[2] / 2.,
                        hit.rect[1] + hit.rect[3] / 2.,
                    ];
                    for draw in &mut view.draws {
                        if draw.node.kind == "image"
                            && draw
                                .node
                                .parents
                                .iter()
                                .any(|p| p == "source_button_background")
                            && warzone_app_client::interface::contains(draw.node.rect, centre)
                        {
                            draw.node.parents.push("source_button_pressed".into());
                        }
                    }
                }
            }
        }
        Some(view)
    }
    pub fn campaign_briefing(&self) -> bool {
        self.mission_briefing
    }
    pub fn special_bought(&mut self, scene: &Scene, state: &State, id: u32) {
        if !matches!(self.kind, Some(Kind::Construction | Kind::Shop)) {
            return;
        }
        if let Some(item) = scene
            .progression
            .as_ref()
            .and_then(|c| c.special_items.get(&id))
        {
            let amount = item
                .outputs
                .iter()
                .find_map(|a| {
                    let key = match a.kind.as_str() {
                        "playMoney" => "#amount_playmoney",
                        "fuel" => "#amount_fuel",
                        "iron" => "#amount_iron",
                        "energy" => "#amount_energy",
                        _ => return None,
                    };
                    let count = if a.count == "max" {
                        state.max_energy(scene)
                    } else {
                        a.count.parse().unwrap_or(0)
                    };
                    Some(warzone_app_client::interface_resources::number_text(
                        scene
                            .progression
                            .as_ref()
                            .and_then(|c| c.language.get(key))
                            .map_or("", String::as_str),
                        count,
                    ))
                })
                .unwrap_or_else(|| item.name.clone());
            self.notice = Some(
                scene
                    .progression
                    .as_ref()
                    .and_then(|c| c.language.get("#purchase_complete"))
                    .map_or("Ты купил [1]!", String::as_str)
                    .replace("[1]", &amount),
            );
        }
    }
    pub fn building_shortfall(&mut self, id: u32) {
        self.build_shortfall = Some(id);
        self.native_press = None;
    }
    pub fn offer_capacity(&mut self, prompt: warzone_app_client::capacity::Prompt) {
        self.kind = None;
        self.native_press = None;
        self.selection = None;
        self.capacity_target = prompt.offer.then_some(prompt.building);
        self.notice = Some(prompt.text);
    }
    pub fn research_bought(&mut self) {
        self.kind = Some(Kind::Research);
        self.research_detail = false;
    }
    pub(crate) fn native_action(
        &mut self,
        action: warzone_app_client::interface::Action,
        scene: &Scene,
        state: &State,
    ) -> EventResult {
        use warzone_app_client::interface::Action as A;
        if self.level_up.is_some() {
            if matches!(action, A::Close | A::Confirm) {
                self.level_up = None;
                self.scroll = 0.;
                self.scroll_drag = None;
                self.scroll_velocity = 0.;
                self.native_press = None;
            }
            return EventResult::Consumed;
        }
        if state.quest_story().ok().flatten().is_some() {
            return if self.story_input_ready() && matches!(action, A::Confirm | A::Primary) {
                EventResult::QuestStoryNext
            } else {
                EventResult::Consumed
            };
        }
        if self.production_shortfall.is_some() {
            if action == A::Confirm {
                return EventResult::ProductionTopup(self.production_shortfall.take().unwrap());
            }
            if matches!(action, A::Dismiss | A::Cancel | A::Close) {
                self.production_shortfall = None;
            }
            return EventResult::Consumed;
        }
        if self.build_shortfall.is_some() {
            if action == A::Confirm {
                self.build_shortfall = None;
                return EventResult::BuildingTopup;
            }
            if matches!(action, A::Dismiss | A::Cancel | A::Close) {
                self.build_shortfall = None;
            }
            return EventResult::Consumed;
        }
        if self.notice.is_some() && self.capacity_target.is_some() {
            if action == A::Confirm {
                let id = self.capacity_target.take().unwrap();
                self.notice = None;
                self.guide_shop(scene, state, id);
            } else if matches!(action, A::Dismiss | A::Cancel | A::Close) {
                self.capacity_target = None;
                self.notice = None;
            }
            return EventResult::Consumed;
        }
        if self.material_info.is_some() {
            if matches!(action, A::Close | A::Dismiss | A::Cancel) {
                self.material_info = None;
            }
            return EventResult::Consumed;
        }
        match action {
            A::TutorialCancel => {
                self.guide = None;
                self.native_press = None;
                return EventResult::Consumed;
            }
            A::Achievements => {
                if scene.achievements.is_some() {
                    self.kind = Some(Kind::Achievements);
                    self.scroll = 0.;
                }
            }
            A::ClaimAchievement(id, tier) => {
                if self.kind == Some(Kind::Achievements) {
                    return EventResult::ClaimAchievement(id, tier);
                }
            }
            A::Bank => {
                self.kind = Some(Kind::Construction);
                self.shop_tab = 1;
                self.scroll = 0.;
                self.native_press = None;
            }
            A::Energy => {
                if state.energy(scene, self.now) >= state.max_energy(scene) {
                    return EventResult::Notice(
                        warzone_app_client::interface_energy::full_message(scene),
                    );
                }
                if warzone_app_client::interface_energy::available(scene) {
                    self.kind = Some(Kind::Energy);
                    self.native_press = None;
                }
            }
            A::EnergyBuy => {
                if self.kind == Some(Kind::Energy) {
                    return EventResult::Buy(4000);
                }
            }
            A::EnergyWait => {
                if self.kind == Some(Kind::Energy) {
                    self.kind = None;
                    self.native_press = None;
                }
            }
            A::BonusPickup(id) => return EventResult::BonusPickup(id),
            A::FallingPickup(sequence, id, instance) => {
                self.reward_motion
                    .pickup(sequence, id, instance, self.animation_seconds);
            }
            A::ShowQuest(q, i) => return EventResult::QuestGuide(q, i),
            A::LandOpen(id) => {
                if state.expansion_quote(scene, id).is_ok() {
                    self.kind = Some(Kind::Land);
                    self.land_id = Some(id);
                    self.land_shortfall = false;
                }
            }
            A::LandCoins | A::LandCash | A::LandTopup => {
                if let Some(id) = self.land_id {
                    if self.kind == Some(Kind::Land) {
                        match state.expansion_quote(scene, id) {
                            Ok(q) if action == A::LandCoins && q.missing > 0 => {
                                self.land_shortfall = true;
                            }
                            Ok(_) => {
                                return EventResult::BuyLand(
                                    id,
                                    match action {
                                        A::LandCoins => {
                                            warzone_app_client::expansions::Payment::Coins
                                        }
                                        A::LandCash => {
                                            warzone_app_client::expansions::Payment::Cash
                                        }
                                        _ => warzone_app_client::expansions::Payment::MissingCoins,
                                    },
                                );
                            }
                            Err(text) => return EventResult::Notice(text),
                        }
                    }
                }
            }
            A::ShopTab(tab) => {
                if let Some(g) = &mut self.guide {
                    if let warzone_app_client::quest_navigation::Route::Building { id, .. } =
                        g.route
                    {
                        if scene
                            .shop
                            .as_ref()
                            .and_then(|c| c.items.get(&id))
                            .is_some_and(|i| i.tab == tab)
                        {
                            g.shop_category_done = true;
                            g.aligned_stage = "tab_changed".into();
                            g.stage.clear();
                        }
                    }
                }
                self.shop_tab = tab;
                self.scroll = 0.;
            }
            A::ShopUnlock(id) => return EventResult::UnlockShop(id),
            A::ShopBuy(id) => {
                // PrepareToQuickBuild has only shop + item. Placement belongs
                // to the ordinary editor, without a surviving tutorial lock.
                if self.guide.as_ref().is_some_and(|g| {
                    g.uid.is_none()
                        && g.route
                            == warzone_app_client::quest_navigation::Route::Building {
                                id,
                                quick: true,
                            }
                }) {
                    self.guide = None;
                }
                if scene
                    .shop
                    .as_ref()
                    .and_then(|c| c.items.get(&id))
                    .is_some_and(|i| i.kind == "special_items")
                {
                    return EventResult::Buy(id);
                }
                return EventResult::PlanBuilding(id);
            }
            A::BankClaim(id) => return EventResult::BankClaim(id),
            A::BuyMaterial(id) => return EventResult::BuyMaterial(id),
            A::MaterialInfo(id) => {
                self.material_info = Some(id);
                self.native_press = None;
            }
            A::BuyResearch(id) => {
                return self.selection.clone().map_or(EventResult::Consumed, |uid| {
                    EventResult::BuyResearch(uid, id)
                });
            }
            A::ResearchAccess(id) => {
                return self.selection.clone().map_or(EventResult::Consumed, |uid| {
                    EventResult::ResearchUnlock(uid, id)
                });
            }
            A::ObjectPrimary(index) => {
                return state
                    .entities
                    .keys()
                    .nth(index)
                    .cloned()
                    .map_or(EventResult::Consumed, EventResult::ObjectPrimary);
            }
            A::ResourceSpeedup => {
                return self
                    .selection
                    .clone()
                    .map_or(EventResult::Consumed, EventResult::ResourceSpeedup);
            }
            A::RecipeAccess(id) => {
                if let Some(uid) = self.selection.clone() {
                    self.recipe_access = Some((uid, id));
                }
            }
            A::Quest(index) => {
                self.kind = Some(Kind::Quests);
                self.index = index;
                self.scroll = 0.;
                if let Some(q) = state.active_quests(scene).get(index) {
                    if state.quest_dialog_needs_commit(scene, q.id) {
                        return EventResult::QuestStoryOpen(q.id);
                    }
                }
            }
            A::ClaimQuest(id) => {
                self.kind = None;
                return EventResult::Claim(id);
            }
            A::Inventory => {
                self.kind = Some(Kind::Inventory);
                self.scroll = 0.;
            }
            A::Settings => {
                self.kind = Some(Kind::Settings);
                self.lang_popup = false;
            }
            A::Construction => {
                self.kind = Some(Kind::Construction);
                self.shop_tab = 2;
                self.scroll = 0.;
                self.index = 0;
                if let Some(Guide {
                    route: warzone_app_client::quest_navigation::Route::Building { id, .. },
                    shop_category_done: true,
                    ..
                }) = &self.guide
                {
                    let id = *id;
                    self.guide_shop(scene, state, id);
                }
            }
            A::Missions => {
                self.kind = Some(Kind::Missions);
                self.index = Self::next_mission(scene, state);
                self.mission_briefing = false;
                self.mission_offset = None;
                if let Some(Guide {
                    route: warzone_app_client::quest_navigation::Route::Mission(id),
                    ..
                }) = &self.guide
                {
                    if let Some((i, m)) = scene
                        .combat
                        .as_ref()
                        .and_then(|c| c.campaign().enumerate().find(|(_, m)| m.mission_id == *id))
                    {
                        self.index = i;
                        self.mission_offset = Some([
                            453. + m.map_position[0] as f64 * 0.5 - 240.,
                            343. + m.map_position[1] as f64 * 0.5 - 160.,
                        ]);
                    }
                }
            }
            A::MissionSelect(index) => {
                self.index = index;
                if state.mission_paused()
                    && scene
                        .combat
                        .as_ref()
                        .and_then(|c| c.campaign().nth(index))
                        .is_some_and(|m| {
                            state
                                .mission
                                .as_ref()
                                .is_some_and(|session| session.battle.mission == m.mission)
                        })
                {
                    return self.action(scene, state);
                }
                if let EventResult::Notice(text) = self.action(scene, state) {
                    return EventResult::Notice(text);
                }
                self.mission_briefing = true;
            }
            A::Close => {
                if self.kind == Some(Kind::LeaguesHelp) {
                    self.kind = Some(Kind::Leagues);
                    self.scroll = 0.;
                    return EventResult::Consumed;
                }
                if self.kind == Some(Kind::Leagues) {
                    self.kind = if self.leagues_from_profile {
                        Some(Kind::Pvp)
                    } else {
                        None
                    };
                    return EventResult::Consumed;
                }
                if self.kind == Some(Kind::Land) {
                    self.kind = None;
                    self.land_id = None;
                    self.land_shortfall = false;
                    return EventResult::Consumed;
                }
                if self.kind == Some(Kind::Missions) && self.mission_briefing {
                    self.mission_briefing = false;
                } else if self.kind == Some(Kind::Upgrades) {
                    self.close_upgrades();
                } else if self.kind == Some(Kind::Research) && self.research_detail {
                    self.research_detail = false;
                } else {
                    self.kind = None;
                }
                self.lang_popup = false;
            }
            A::Music => return EventResult::ToggleAudio("playMusic".into()),
            A::Sound => return EventResult::ToggleAudio("playSound".into()),
            A::Move => return EventResult::Key(Keycode::M),
            A::Rotate => return EventResult::Key(Keycode::R),
            A::Sell => {
                if let Some(uid) = self
                    .selection
                    .as_deref()
                    .filter(|uid| state.can_clear_obstacle(scene, uid).is_ok())
                {
                    return EventResult::ClearObstacle(uid.into());
                }
                return EventResult::Key(Keycode::Delete);
            }
            A::UpgradeUnit(unit) => {
                if warzone_app_client::interface_upgrades::researched(scene, state, unit) {
                    self.upgrade_unit = Some(unit);
                    self.upgrade_detail = None;
                    self.scroll = 0.;
                } else if let Some((uid, _)) = state
                    .entities
                    .iter()
                    .find(|(_, e)| e["type"] == 1020 && e["state"] == 3 && e["isDeleted"] != true)
                {
                    return EventResult::AcademyResearch(uid.clone(), unit);
                } else {
                    self.capacity_target = Some(1020);
                    self.notice=Some(warzone_app_client::interface::lang(scene,"#no_research_building","Чтобы проводить исследования нужен Исследовательский Центр. Нажмите \"Показать\", чтобы перейти в магазин и построить его.").into());
                    return EventResult::Consumed;
                }
            }
            A::UpgradeOpen(id) | A::UpgradeInfo(id) => {
                self.upgrade_detail = Some(id);
                self.upgrade_info = matches!(action, A::UpgradeInfo(_));
                self.scroll_velocity = 0.;
            }
            A::UpgradeStart(id) => {
                if let Some(cmd) = self
                    .selection
                    .as_deref()
                    .and_then(|uid| state.upgrade_command(uid, id))
                {
                    return EventResult::UpgradeApply(cmd);
                }
            }
            A::UpgradeBuy(id) => {
                if let Some(uid) = &self.selection {
                    return EventResult::BuyUpgrade(uid.clone(), id);
                }
            }
            A::Primary => {
                if matches!(self.kind, Some(Kind::Missions | Kind::Pvp)) {
                    return self.action(scene, state);
                }
                if self.selection.as_deref().is_some_and(|uid| {
                    scene
                        .interface
                        .as_ref()
                        .is_some_and(|c| c.symbols.contains_key("production_speedup"))
                        && scene.economy.as_ref().is_some_and(|c| {
                            self.now.checked_sub(c.time_shift).is_some_and(|wall| {
                                state.production_speedup_quote(scene, uid, wall).is_ok()
                            })
                        })
                }) {
                    self.kind = Some(Kind::ResourceSpeedup);
                } else if self
                    .selection
                    .as_deref()
                    .and_then(|uid| state.entities.get(uid))
                    .is_some_and(|e| {
                        e["type"] == 1019 && e["state"] == 3 && e["productionType"] == 0
                    })
                    && scene.upgrades.is_some()
                {
                    self.kind = Some(Kind::Upgrades);
                    self.upgrade_unit = None;
                    self.upgrade_detail = None;
                    self.scroll = 30. * self.viewport_scale;
                } else if self.selection.as_deref().is_some_and(|uid| {
                    state.research_choice(scene, uid, 0).is_some()
                        && !state.repair_pending(scene, uid)
                        && !state.construction_pending(scene, uid)
                }) {
                    self.kind = Some(Kind::Research);
                    self.research_detail = false;
                    self.index = 0;
                    if let Some(id) = self
                        .selection
                        .as_deref()
                        .and_then(|uid| state.entities.get(uid))
                        .and_then(|e| e["productionType"].as_u64())
                        .filter(|id| *id != 0)
                    {
                        if let Some(i) = warzone_app_client::interface_windows::RESEARCH_ORDER
                            .iter()
                            .position(|r| *r == id as u32)
                        {
                            self.index = i;
                            self.research_detail = true;
                        }
                    }
                } else if self.selection.as_deref().is_some_and(|uid| {
                    !state.repair_pending(scene, uid)
                        && !state.construction_pending(scene, uid)
                        && state.workshop_choice(scene, uid, 0).is_some()
                }) {
                    self.kind = Some(Kind::Production);
                    self.scroll = 0.;
                } else if self.selection.as_deref().is_some_and(|uid| {
                    scene
                        .interface
                        .as_ref()
                        .is_some_and(|c| c.symbols.contains_key("production_speedup"))
                        && scene.economy.as_ref().is_some_and(|c| {
                            self.now.checked_sub(c.time_shift).is_some_and(|wall| {
                                state.resource_speedup_quote(scene, uid, wall).is_ok()
                            })
                        })
                }) {
                    self.kind = Some(Kind::ResourceSpeedup);
                } else if self.selection.as_deref().is_some_and(|uid| {
                    state
                        .entities
                        .get(uid)
                        .is_some_and(|e| e["productionType"] == 0)
                        && scene
                            .interface
                            .as_ref()
                            .is_some_and(|c| c.symbols.contains_key("resource_production"))
                        && !state.repair_pending(scene, uid)
                        && !state.construction_pending(scene, uid)
                        && scene
                            .economy
                            .as_ref()
                            .and_then(|c| state.command(c, uid, 0))
                            .is_some()
                }) {
                    self.kind = Some(Kind::ResourceProduction);
                } else {
                    return EventResult::Key(Keycode::Return);
                }
                // Generic collect_* with itemId=0 guides to the producer's
                // menu only; the player chooses the contract themselves.
                if matches!(self.kind, Some(Kind::ResourceProduction | Kind::Production))
                    && self
                        .guide
                        .as_ref()
                        .is_some_and(|g| g.menu_only && g.uid == self.selection)
                {
                    self.guide = None;
                }
            }
            A::Produce(index) => {
                self.kind = None;
                return EventResult::Production(index);
            }
            A::Confirm => {
                if self.kind == Some(Kind::Sell) {
                    return self.confirm_sale(scene, state);
                }
                if let Some((uid, id)) = self.recipe_access.take() {
                    return EventResult::RecipeUnlock(uid, id);
                }
                return EventResult::Key(Keycode::Return);
            }
            A::Dismiss => {
                self.notice = None;
                self.recipe_access = None;
                if self.kind == Some(Kind::Sell) {
                    return self.cancel_sale();
                }
            }
            A::Previous => {
                self.index = self.index.saturating_sub(1);
            }
            A::Next => {
                self.index = self.index.saturating_add(1);
            }
            A::Choose(index) => {
                // PrepareToResearch ends at the research cell, before the
                // detail's paid Start action (Tutorials 0x13ba29).
                if self.kind == Some(Kind::Research)
                    && self.guide.as_ref().is_some_and(|g| {
                        matches!(g.route,warzone_app_client::quest_navigation::Route::Research(id)
                        if warzone_app_client::interface_windows::RESEARCH_ORDER.get(index)==Some(&id))
                    })
                {
                    self.guide = None;
                }
                self.index = index;
                self.research_detail = true;
            }
            A::Cancel => return EventResult::Key(Keycode::Escape),
            A::Language => self.lang_popup = !self.lang_popup,
            A::Pvp => {
                if scene.combat.as_ref().is_some_and(|c| c.pvp.is_some()) {
                    self.kind = Some(Kind::Pvp);
                    self.scroll = 0.;
                } else {
                    return EventResult::Notice("Режим сражений ещё недоступен".into());
                }
            }
            A::PvpHistory => return EventResult::Notice("Нет подключения к серверу".into()),
            A::PvpLeagues => {
                if scene
                    .interface
                    .as_ref()
                    .is_some_and(|c| c.symbols.contains_key("leagues"))
                {
                    self.leagues_from_profile = self.kind == Some(Kind::Pvp);
                    self.kind = Some(Kind::Leagues);
                    self.league_index = warzone_app_client::interface_leagues::current(state);
                    self.scroll = 0.;
                } else {
                    return EventResult::Notice("Нет подключения к серверу".into());
                }
            }
            A::LeagueSelect(index) => {
                if self.kind == Some(Kind::Leagues) && index < 4 {
                    self.league_index = index;
                }
            }
            A::LeaguesHelp => {
                if self.kind == Some(Kind::Leagues) {
                    self.kind = Some(Kind::LeaguesHelp);
                    self.scroll = 0.;
                }
            }
            A::Friends => {
                return EventResult::Notice(
                    "Друзья будут доступны после подключения онлайн-режима".into(),
                );
            }
            A::Facebook | A::GameCenter => {
                return EventResult::Notice("Нет подключения к сервису".into());
            }
            A::Forum => return EventResult::Notice("Форум игры недоступен".into()),
            A::Support => return EventResult::Notice("Нет подключения к службе поддержки".into()),
            A::English | A::Russian | A::Chinese => {
                self.lang_popup = false;
                return EventResult::SetLanguage(match action {
                    A::English => 0,
                    A::Russian => 1,
                    _ => 2,
                });
            }
        }
        EventResult::Consumed
    }
    fn count(&self, scene: &Scene, state: &State) -> usize {
        match self.kind {
            Some(Kind::Settings) => 2,
            Some(Kind::Missions) => scene.combat.as_ref().map_or(0, |c| c.campaign().count()),
            Some(Kind::Quests) => state.active_quests(scene).len(),
            Some(Kind::Shop) => state.shop_items(scene).len(),
            Some(Kind::Construction) => state.building_items(scene).len(),
            Some(Kind::Inventory) => state
                .inventory
                .values()
                .filter(|n| **n > 0)
                .count()
                .div_ceil(5)
                .max(1),
            _ => 1,
        }
    }
    fn action(&self, scene: &Scene, state: &State) -> EventResult {
        match self.kind {
            Some(Kind::Pvp) => EventResult::OpenMission("pvptutorial_1".into()),
            Some(Kind::Settings) => EventResult::ToggleAudio(
                if self.index.is_multiple_of(2) {
                    "playMusic"
                } else {
                    "playSound"
                }
                .into(),
            ),
            Some(Kind::Missions) => scene
                .combat
                .as_ref()
                .and_then(|c| {
                    c.campaign_names()
                        .nth(self.index % c.campaign().count().max(1))
                })
                .map_or(EventResult::Consumed, |k| {
                    EventResult::OpenMission(k.clone())
                }),
            Some(Kind::Sell) => self
                .sell_uid
                .as_ref()
                .map_or(EventResult::Consumed, |uid| EventResult::Sell(uid.clone())),
            Some(Kind::Construction) => {
                let items = state.building_items(scene);
                items
                    .get(self.index % items.len().max(1))
                    .map_or(EventResult::Consumed, |b| EventResult::PlanBuilding(b.id))
            }
            Some(Kind::Quests) => {
                let ids = state.active_quests(scene);
                ids.get(self.index % ids.len().max(1))
                    .map_or(EventResult::Consumed, |q| EventResult::Claim(q.id))
            }
            Some(Kind::Shop) => {
                let ids = state.shop_items(scene);
                ids.get(self.index % ids.len().max(1))
                    .map_or(EventResult::Consumed, |q| EventResult::Buy(q.id))
            }
            _ => EventResult::Consumed,
        }
    }
    fn event_view(
        &self,
        event: &Event,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) -> Option<warzone_app_client::interface::View> {
        // HUD/window hits precede map markers. Do not build hundreds of marker
        // drawings just to discover that the pointer is over Inventory/Settings.
        let empty = warzone_app_client::interface::View::default();
        let mut view = self.original_view_with_markers(scene, state, camera, Some(&empty))?;
        if self.kind.is_none() && !view.modal {
            if let Event::MouseButtonDown { x, y, .. } | Event::MouseButtonUp { x, y, .. } = event {
                if view.hit(*x, *y).is_none() {
                    view.hits
                        .splice(0..0, self.world_markers(scene, state, camera).hits);
                }
            }
        }
        Some(view)
    }
    pub fn event(
        &mut self,
        event: &Event,
        camera: &Camera,
        scene: &Scene,
        state: &State,
    ) -> EventResult {
        self.viewport_scale = warzone_app_client::hud::scale(camera);
        match event {
            Event::MouseButtonDown { x, y, .. }
            | Event::MouseButtonUp { x, y, .. }
            | Event::MouseMotion { x, y, .. } => self.native_pointer = [*x as f64, *y as f64],
            _ => {}
        }
        if self.animate_bottom
            && matches!(
                event,
                Event::MouseButtonDown { .. }
                    | Event::MouseButtonUp { .. }
                    | Event::MouseMotion { .. }
            )
            && self
                .bottom_motion
                .borrow()
                .blocks_point(self.animation_seconds, self.native_pointer)
        {
            return EventResult::Consumed;
        }
        self.story_motion
            .observe(scene, state, true, self.animation_seconds);
        if self.level_up.is_some() {
            use warzone_app_client::interface::Action as A;
            return match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return | Keycode::Escape | Keycode::Space),
                    repeat: false,
                    ..
                } => self.native_action(A::Close, scene, state),
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    let v = self.original_view(scene, state, camera);
                    self.scroll_maximum = v.as_ref().map_or(0., |v| v.scroll_max);
                    self.native_press = v.and_then(|v| v.hit(*x, *y));
                    if self.native_press.is_none() {
                        self.scroll_drag = Some((*x, self.scroll));
                        self.scroll_velocity = 0.;
                    }
                    EventResult::Consumed
                }
                Event::MouseMotion { x, .. } => {
                    if let Some((start, offset)) = self.scroll_drag {
                        self.scroll = warzone_app_client::scroll_motion::rubber(
                            offset + (start - *x) as f64,
                            self.scroll_maximum,
                            camera.width as f64,
                        );
                    }
                    EventResult::Consumed
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if self.scroll_drag.take().is_some() {
                        return EventResult::Consumed;
                    }
                    let hit = self
                        .original_view(scene, state, camera)
                        .and_then(|v| v.hit(*x, *y));
                    let pressed = self.native_press.take();
                    if let Some(action) = hit.filter(|_| hit == pressed) {
                        self.button_taps += 1;
                        self.native_action(action, scene, state)
                    } else {
                        EventResult::Consumed
                    }
                }
                Event::MouseWheel { x, y, .. } => {
                    let maximum = self
                        .original_view(scene, state, camera)
                        .map_or(0., |v| v.scroll_max);
                    self.scroll_maximum = maximum;
                    self.scroll =
                        (self.scroll + ((*x as f64) - (*y as f64)) * 60.).clamp(0., maximum);
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            };
        }
        if self.quest_scrolling() {
            if matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    ..
                }
            ) {
                self.guide = None;
                return EventResult::Consumed;
            }
            if matches!(
                event,
                Event::MouseButtonDown { .. }
                    | Event::MouseButtonUp { .. }
                    | Event::MouseMotion { .. }
                    | Event::MouseWheel { .. }
                    | Event::KeyDown { .. }
                    | Event::KeyUp { .. }
            ) {
                return EventResult::Consumed;
            }
        }

        if self.training_intro.is_some() {
            return match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return | Keycode::Space),
                    repeat: false,
                    ..
                }
                | Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    let (source, id) = self.training_intro.take().unwrap();
                    EventResult::ObjectTraining(source, id)
                }
                _ => EventResult::Consumed,
            };
        }
        if self.guide_pointer(event, scene, state, camera) {
            return EventResult::Consumed;
        }
        if state.quest_story().ok().flatten().is_some() {
            if !self.story_input_ready() {
                return EventResult::Consumed;
            }
            return match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Return | Keycode::Space),
                    repeat: false,
                    ..
                } => EventResult::QuestStoryNext,
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    self.native_press = Some(warzone_app_client::interface::Action::Confirm);
                    EventResult::Consumed
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    if self.native_press.take().is_some() {
                        EventResult::QuestStoryNext
                    } else {
                        EventResult::Consumed
                    }
                }
                _ => EventResult::Consumed,
            };
        }

        if self.material_info.is_some()
            && matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape | Keycode::Return),
                    repeat: false,
                    ..
                }
            )
        {
            self.material_info = None;
            return EventResult::Consumed;
        }
        if (self.build_shortfall.is_some() || self.production_shortfall.is_some())
            && matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Return | Keycode::Escape),
                    repeat: false,
                    ..
                }
            )
        {
            return self.native_action(
                if matches!(
                    event,
                    Event::KeyDown {
                        keycode: Some(Keycode::Return),
                        ..
                    }
                ) {
                    warzone_app_client::interface::Action::Confirm
                } else {
                    warzone_app_client::interface::Action::Dismiss
                },
                scene,
                state,
            );
        }
        if self.recipe_access.is_some() {
            if matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    ..
                }
            ) {
                self.recipe_access = None;
                return EventResult::Consumed;
            }
            if matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    ..
                }
            ) {
                return self.native_action(
                    warzone_app_client::interface::Action::Confirm,
                    scene,
                    state,
                );
            }
        }
        if self.notice.is_some()
            && matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape | Keycode::Return),
                    repeat: false,
                    ..
                }
            )
        {
            if self.capacity_target.is_some() {
                return self.native_action(
                    if matches!(
                        event,
                        Event::KeyDown {
                            keycode: Some(Keycode::Return),
                            ..
                        }
                    ) {
                        warzone_app_client::interface::Action::Confirm
                    } else {
                        warzone_app_client::interface::Action::Dismiss
                    },
                    scene,
                    state,
                );
            }
            self.notice = None;
            return EventResult::Consumed;
        }
        let requested = if self.notice.is_some()
            || self.recipe_access.is_some()
            || self.build_shortfall.is_some()
            || self.production_shortfall.is_some()
        {
            None
        } else {
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::F2),
                    repeat: false,
                    ..
                } => Some(Kind::Settings),
                Event::KeyDown {
                    keycode: Some(Keycode::F5),
                    repeat: false,
                    ..
                } => Some(Kind::Missions),
                Event::KeyDown {
                    keycode: Some(Keycode::C),
                    repeat: false,
                    ..
                } => Some(Kind::Construction),
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    repeat: false,
                    ..
                } => Some(Kind::Quests),
                Event::KeyDown {
                    keycode: Some(Keycode::I),
                    repeat: false,
                    ..
                } => Some(Kind::Inventory),
                Event::KeyDown {
                    keycode: Some(Keycode::B),
                    repeat: false,
                    ..
                } => Some(Kind::Shop),
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } if scene.interface.is_none() => {
                    crate::render::navigation_hit(scene, camera, *x, *y)
                }
                _ => None,
            }
        };
        if let Some(kind) = requested {
            if self.kind == Some(Kind::Sell) {
                return EventResult::Consumed;
            }
            self.kind = if self.kind == Some(kind) {
                None
            } else {
                Some(kind)
            };
            self.shop_tab = if kind == Kind::Shop { 7 } else { 2 };
            if kind == Kind::Missions && self.kind.is_some() {
                self.index = Self::next_mission(scene, state);
                self.mission_briefing = false;
                self.mission_offset = None;
            }
            self.pressed = None;
            self.native_press = None;
            self.scroll = 0.;
            self.lang_popup = false;
            if kind == Kind::Quests && self.kind.is_some() {
                let qs = state.active_quests(scene);
                if let Some(q) = qs.get(self.index % qs.len().max(1)) {
                    if state.quest_dialog_needs_commit(scene, q.id) {
                        return EventResult::QuestStoryOpen(q.id);
                    }
                }
            }
            return EventResult::Consumed;
        }
        if scene.interface.is_some()
            && (self.notice.is_some()
                || self.recipe_access.is_some()
                || (self.kind == Some(Kind::Missions)
                    && warzone_app_client::interface_campaign::available(scene))
                || self.kind.is_none()
                || (scene.shop.is_some()
                    && matches!(self.kind, Some(Kind::Construction | Kind::Shop)))
                || (self.kind == Some(Kind::Quests)
                    && scene
                        .interface
                        .as_ref()
                        .is_some_and(|c| c.symbols.contains_key("quests")))
                || matches!(
                    self.kind,
                    Some(
                        Kind::Land
                            | Kind::Sell
                            | Kind::Achievements
                            | Kind::Leagues
                            | Kind::LeaguesHelp
                            | Kind::Energy
                            | Kind::Pvp
                            | Kind::Inventory
                            | Kind::Settings
                            | Kind::Production
                            | Kind::Research
                            | Kind::Upgrades
                            | Kind::ResourceProduction
                            | Kind::ResourceSpeedup
                    )
                ))
        {
            if self.kind == Some(Kind::Missions) && self.notice.is_none() {
                match event {
                    Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        repeat: false,
                        ..
                    } => {
                        return self.native_action(
                            warzone_app_client::interface::Action::Close,
                            scene,
                            state,
                        );
                    }
                    Event::KeyDown {
                        keycode: Some(Keycode::Return),
                        repeat: false,
                        ..
                    } => {
                        if self.mission_briefing {
                            let allowed = scene
                                .combat
                                .as_ref()
                                .and_then(|c| c.campaign().nth(self.index))
                                .is_some_and(|m| state.mission_access(scene, &m.mission).is_ok());
                            return if allowed {
                                self.action(scene, state)
                            } else {
                                EventResult::Consumed
                            };
                        }
                        return self.native_action(
                            warzone_app_client::interface::Action::MissionSelect(self.index),
                            scene,
                            state,
                        );
                    }
                    Event::KeyDown {
                        keycode: Some(key @ (Keycode::Left | Keycode::Right | Keycode::Tab)),
                        repeat: false,
                        ..
                    } => {
                        let n = self.count(scene, state).max(1);
                        self.index =
                            (self.index + if *key == Keycode::Left { n - 1 } else { 1 }) % n;
                        return EventResult::Consumed;
                    }
                    _ => {}
                }
            }
            let view = self.event_view(event, scene, state, camera).unwrap();
            self.scroll_maximum = view.scroll_max;
            let scroll_allowed = self.notice.is_none()
                && !(self.kind == Some(Kind::Upgrades) && self.upgrade_detail.is_some());
            if self.kind == Some(Kind::Sell)
                && matches!(
                    event,
                    Event::KeyDown {
                        keycode: Some(Keycode::Return),
                        repeat: false,
                        ..
                    }
                )
            {
                return self.confirm_sale(scene, state);
            }
            match event {
                Event::Quit { .. } | Event::Window { .. } => return EventResult::Pass,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    ..
                } if view.modal => {
                    if self.kind == Some(Kind::Sell) {
                        return self.cancel_sale();
                    }
                    if matches!(self.kind, Some(Kind::Leagues | Kind::LeaguesHelp)) {
                        return self.native_action(
                            warzone_app_client::interface::Action::Close,
                            scene,
                            state,
                        );
                    }
                    if self.kind == Some(Kind::Upgrades) {
                        self.close_upgrades();
                    } else if self.kind == Some(Kind::Research) && self.research_detail {
                        self.research_detail = false;
                    } else {
                        self.kind = None;
                    }
                    self.lang_popup = false;
                    return EventResult::Consumed;
                }
                Event::MouseWheel { y, .. }
                    if matches!(
                        self.kind,
                        Some(
                            Kind::Inventory
                                | Kind::Production
                                | Kind::Quests
                                | Kind::LeaguesHelp
                                | Kind::Achievements
                                | Kind::Construction
                                | Kind::Shop
                                | Kind::Upgrades
                        )
                    ) && scroll_allowed =>
                {
                    let distance = if self.kind == Some(Kind::LeaguesHelp) {
                        warzone_app_client::interface_leagues::page_width(scene, camera)
                            .unwrap_or(50.)
                    } else {
                        50.
                    };
                    if self.kind == Some(Kind::LeaguesHelp) {
                        self.scroll_page_target = Some(
                            (self.scroll_page_target.unwrap_or(self.scroll) - *y as f64 * distance)
                                .clamp(0., view.scroll_max),
                        );
                        self.scroll_velocity = 0.;
                    } else {
                        self.scroll =
                            (self.scroll - *y as f64 * distance).clamp(0., view.scroll_max);
                    }
                    return EventResult::Consumed;
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    self.native_press = view.hit(*x, *y);
                    if self.native_press.is_some() {
                        self.button_taps += 1;
                    }
                    if matches!(
                        self.kind,
                        Some(
                            Kind::Inventory
                                | Kind::Production
                                | Kind::Quests
                                | Kind::LeaguesHelp
                                | Kind::Achievements
                                | Kind::Construction
                                | Kind::Shop
                                | Kind::Upgrades
                        )
                    ) && scroll_allowed
                        && view.draws.iter().filter_map(|d| d.clip).any(|r| {
                            warzone_app_client::interface::contains(r, [*x as f64, *y as f64])
                        })
                        && (self.kind != Some(Kind::LeaguesHelp)
                            || warzone_app_client::interface_leagues::clip(scene, camera)
                                .is_some_and(|r| {
                                    warzone_app_client::interface::contains(
                                        r,
                                        [*x as f64, *y as f64],
                                    )
                                }))
                        && (self.kind != Some(Kind::Achievements)
                            || view
                                .draws
                                .iter()
                                .find(|d| d.node.id == "achiev_title")
                                .and_then(|d| d.clip)
                                .is_some_and(|r| {
                                    warzone_app_client::interface::contains(
                                        r,
                                        [*x as f64, *y as f64],
                                    )
                                }))
                    {
                        self.scroll_drag = Some((
                            if matches!(self.kind, Some(Kind::Quests | Kind::Achievements)) {
                                *y
                            } else {
                                *x
                            },
                            self.scroll,
                        ));
                        self.scroll_velocity = 0.;
                        self.scroll_page_target = None;
                        self.scroll_sample = Some((std::time::Instant::now(), self.scroll));
                    }
                    if self.kind == Some(Kind::Missions)
                        && !self.mission_briefing
                        && self.notice.is_none()
                    {
                        self.mission_drag = Some((
                            *x,
                            *y,
                            self.mission_offset.unwrap_or_else(
                                warzone_app_client::interface_campaign::initial_offset,
                            ),
                        ));
                    }
                    if self.native_press.is_some()
                        || self.scroll_drag.is_some()
                        || view.blocks(*x, *y)
                    {
                        return EventResult::Consumed;
                    }
                }
                Event::MouseMotion { x, y, .. }
                    if self.kind == Some(Kind::Missions)
                        && !self.mission_briefing
                        && self.notice.is_none() =>
                {
                    if let Some((sx, sy, offset)) = self.mission_drag {
                        if (*x - sx).abs() + (*y - sy).abs() > crate::input_events::slop(5) {
                            let scale = warzone_app_client::hud::scale(camera);
                            self.mission_offset =
                                Some(warzone_app_client::interface_campaign::clamp_offset(
                                    camera,
                                    [
                                        offset[0] + (sx - *x) as f64 / scale,
                                        offset[1] + (sy - *y) as f64 / scale,
                                    ],
                                ));
                            self.native_press = None;
                        }
                    }
                    return EventResult::Consumed;
                }
                Event::MouseMotion { x, y, .. }
                    if matches!(
                        self.kind,
                        Some(
                            Kind::Inventory
                                | Kind::Production
                                | Kind::Quests
                                | Kind::LeaguesHelp
                                | Kind::Achievements
                                | Kind::Construction
                                | Kind::Shop
                                | Kind::Upgrades
                        )
                    ) && scroll_allowed =>
                {
                    if let Some((start, scroll)) = self.scroll_drag {
                        let delta = start
                            - if matches!(self.kind, Some(Kind::Quests | Kind::Achievements)) {
                                *y
                            } else {
                                *x
                            };
                        if delta.abs() > crate::input_events::slop(5) {
                            self.native_press = None;
                            let extent =
                                if matches!(self.kind, Some(Kind::Quests | Kind::Achievements)) {
                                    camera.height
                                } else {
                                    camera.width
                                } as f64;
                            self.scroll = warzone_app_client::scroll_motion::rubber(
                                scroll + delta as f64,
                                view.scroll_max,
                                extent,
                            );
                            let now = std::time::Instant::now();
                            if let Some((at, last)) = self.scroll_sample {
                                let dt = now.duration_since(at).as_secs_f64();
                                if dt >= 1. / 240. {
                                    self.scroll_velocity =
                                        ((self.scroll - last) / dt).clamp(-6000., 6000.);
                                    self.scroll_sample = Some((now, self.scroll));
                                }
                            }
                        }
                    }
                    return EventResult::Consumed;
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if self.kind == Some(Kind::LeaguesHelp) && self.scroll_drag.is_some() {
                        if let Some(w) =
                            warzone_app_client::interface_leagues::page_width(scene, camera)
                        {
                            self.scroll_page_target =
                                Some((self.scroll / w).round().clamp(0., 4.) * w);
                            self.scroll_velocity = 0.;
                        }
                    }
                    self.scroll_drag = None;
                    if self
                        .scroll_sample
                        .is_some_and(|(t, _)| t.elapsed().as_secs_f64() > 0.12)
                    {
                        self.scroll_velocity = 0.;
                    }
                    self.scroll_sample = None;
                    self.mission_drag = None;
                    let pressed = self.native_press.take();
                    if let Some(action) = pressed.filter(|_| pressed == view.hit(*x, *y)) {
                        return self.native_action(action, scene, state);
                    }
                    if pressed.is_some() || view.blocks(*x, *y) {
                        return EventResult::Consumed;
                    }
                }
                _ => {}
            }
            if view.modal {
                return EventResult::Consumed;
            }
            let point = match event {
                Event::MouseButtonDown { x, y, .. }
                | Event::MouseButtonUp { x, y, .. }
                | Event::MouseMotion { x, y, .. } => Some((*x, *y)),
                _ => None,
            };
            if point
                .is_some_and(|(x, y)| warzone_app_client::hud::blocks_point(scene, camera, x, y))
            {
                return EventResult::Consumed;
            }
            return EventResult::Pass;
        }
        if self.kind.is_none() {
            let y = match event {
                Event::MouseButtonDown { y, .. }
                | Event::MouseButtonUp { y, .. }
                | Event::MouseMotion { y, .. } => Some(*y),
                _ => None,
            };
            if y.is_some_and(|y| y >= 0 && y < warzone_app_client::hud::height(scene, camera)) {
                return EventResult::Consumed;
            }
            return EventResult::Pass;
        }
        let n = self.count(scene, state).max(1);
        let l = modal_layout(camera);
        let hit = |x, y| {
            let p = Point::new(x, y);
            if l.close.contains_point(p) {
                Some(0)
            } else if l.previous.contains_point(p) {
                Some(1)
            } else if l.next.contains_point(p) {
                Some(2)
            } else if l.claim.contains_point(p) {
                Some(3)
            } else {
                None
            }
        };
        match event {
            Event::Quit { .. } | Event::Window { .. } => return EventResult::Pass,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                ..
            } => {
                self.kind = None;
            }
            Event::KeyDown {
                keycode: Some(Keycode::Left),
                repeat: false,
                ..
            } => self.index = (self.index % n + n - 1) % n,
            Event::KeyDown {
                keycode: Some(Keycode::Right),
                repeat: false,
                ..
            } => self.index = (self.index % n + 1) % n,
            Event::KeyDown {
                keycode: Some(Keycode::Return),
                repeat: false,
                ..
            } => return self.action(scene, state),
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => self.pressed = hit(*x, *y),
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let pressed = self.pressed.take();
                if pressed.is_some() && pressed == hit(*x, *y) {
                    match pressed {
                        Some(0) => self.kind = None,
                        Some(1) => self.index = (self.index % n + n - 1) % n,
                        Some(2) => self.index = (self.index % n + 1) % n,
                        Some(3) => return self.action(scene, state),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        EventResult::Consumed
    }
    pub fn view(&self, scene: &Scene, state: &State, now: i64) -> Option<PanelView> {
        // A retained MessageBox owns the modal layer. Legacy panels must not
        // be painted over it (campaign panels have not yet been replaced).
        if self.notice.is_some() && scene.interface.is_some() {
            return None;
        }
        let kind = self.kind?;
        if kind == Kind::Missions && warzone_app_client::interface_campaign::available(scene) {
            return None;
        }
        if scene.shop.is_some() && matches!(kind, Kind::Construction | Kind::Shop) {
            return None;
        }
        if kind == Kind::Quests
            && scene
                .interface
                .as_ref()
                .is_some_and(|c| c.symbols.contains_key("quests"))
        {
            return None;
        }
        if scene.interface.is_some()
            && matches!(
                kind,
                Kind::Land
                    | Kind::Sell
                    | Kind::Inventory
                    | Kind::Settings
                    | Kind::Production
                    | Kind::Research
                    | Kind::Upgrades
                    | Kind::ResourceProduction
                    | Kind::ResourceSpeedup
            )
        {
            return None;
        }
        let base = PanelView {
            section: String::new(),
            title: String::new(),
            rows: vec![],
            awards: String::new(),
            index: 0,
            total: 0,
            ready: false,
            primary_label: String::new(),
        };
        Some(match kind {
            Kind::Pvp | Kind::Energy | Kind::Achievements | Kind::Leagues | Kind::LeaguesHelp => {
                return None;
            }
            Kind::Settings => {
                let music = self.index.is_multiple_of(2);
                let key = if music { "playMusic" } else { "playSound" };
                PanelView{section:"Настройки".into(),title:if music{"Музыка"}else{"Звуки"}.into(),
                    rows:vec![format!("Сейчас: {}",if state.audio_enabled(key){"включено"}else{"выключено"}),
                        "Изменения сохраняются сразу. F6 — музыка, F7 — звуки, в том числе во время боя.".into()],
                    primary_label:if state.audio_enabled(key){"Выключить · Enter"}else{"Включить · Enter"}.into(),
                    index:self.index%2+1,total:2,ready:true,..base}
            }
            Kind::Missions => {
                let mission = scene
                    .combat
                    .as_ref()
                    .and_then(|c| c.campaign().nth(self.index % c.campaign().count().max(1)));
                if let Some(m) = mission {
                    let replay = state
                        .systems
                        .as_ref()
                        .is_some_and(|p| p.flag(&format!("Mission-{}", m.mission_id)));
                    PanelView {
                        section: "Миссии".into(),
                        title: m.title.clone(),
                        rows: vec![
                            format!(
                                "Армия: {} доступных отрядов · Нужно: {}",
                                state.mission_roster(scene).len(),
                                m.min_descent
                            ),
                            "Перед боем выбери отряды и размести их в зоне высадки.".into(),
                            "Каждое действие требует энергии. Потери отрядов сохраняются.".into(),
                            if replay {
                                "Повторное прохождение. Награда за первую победу уже получена. Энергия и потери учитываются.".into()
                            } else {
                                m.awards
                                    .iter()
                                    .map(|a| format!("{} {}", a.count, resource_name(&a.kind)))
                                    .collect::<Vec<_>>()
                                    .join(" · ")
                            },
                        ],
                        index: self.index
                            % scene.combat.as_ref().unwrap().campaign().count().max(1)
                            + 1,
                        total: scene.combat.as_ref().unwrap().campaign().count(),
                        awards: state
                            .mission_access(scene, &m.mission)
                            .err()
                            .unwrap_or_default(),
                        ready: state.mission_access(scene, &m.mission).is_ok()
                            && state.mission_roster(scene).len() >= m.min_descent,
                        primary_label: "Высадка · Enter".into(),
                    }
                } else {
                    PanelView {
                        section: "Миссии".into(),
                        title: "В пакете нет доступных миссий".into(),
                        ..base
                    }
                }
            }
            Kind::Sell => {
                let uid = self.sell_uid.as_deref().unwrap_or("");
                let quote = state.sale_quote(scene, uid);
                let name = state
                    .entities
                    .get(uid)
                    .and_then(|v| {
                        scene
                            .world
                            .as_ref()?
                            .catalog
                            .get(&(v["type"].as_u64()? as u32))
                    })
                    .map_or("Объект", |d| d.name.as_str());
                PanelView {
                    section: "Продажа объекта".into(),
                    title: format!("Продать: {name}?"),
                    rows: vec![
                        "Объект исчезнет с базы после подтверждения.".into(),
                        "Текущее производство будет потеряно.".into(),
                        "Вместимость и лимит зданий пересчитываются сразу.".into(),
                        "Esc — отменить продажу".into(),
                    ],
                    awards: quote
                        .as_ref()
                        .map_or_else(|e| e.clone(), |p| format!("Возврат: {p} монет")),
                    ready: quote.is_ok(),
                    primary_label: "Продать · Enter".into(),
                    ..base
                }
            }
            Kind::Construction => {
                let items = state.building_items(scene);
                let n = items.len();
                if let Some(b) = items.get(self.index % n.max(1)) {
                    let name = scene.world.as_ref().unwrap().catalog[&b.id].name.clone();
                    let reason = state.building_offer(scene, b.id).err();
                    PanelView {
                        section: "Строительство".into(),
                        title: name,
                        index: self.index % n + 1,
                        total: n,
                        rows: vec![
                            format!(
                                "Уровень: {} · Нужно: {}",
                                state.resources["level"], b.min_level
                            ),
                            format!(
                                "Построено и строится: {} · Доступно: {}",
                                state.building_count(b.id),
                                state.building_access(b)
                            ),
                            format!(
                                "Время строительства: {} сек.",
                                b.seconds(state.building_count(b.id))
                            ),
                            reason.clone().unwrap_or_else(|| {
                                "Выбери место на базе; оплата при подтверждении".into()
                            }),
                        ],
                        awards: format!(
                            "Цена: {}",
                            b.cost
                                .iter()
                                .map(|(k, v)| format!("{v} {}", resource_name(k)))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        ready: reason.is_none(),
                        primary_label: reason.unwrap_or_else(|| "Выбрать место · Enter".into()),
                    }
                } else {
                    PanelView {
                        section: "Строительство".into(),
                        title: "Нет доступных зданий".into(),
                        ..base
                    }
                }
            }
            Kind::Quests => {
                let active = state.active_quests(scene);
                let n = active.len();
                if let Some(q) = active.get(self.index % n.max(1)) {
                    PanelView {
                        section: "Задания".into(),
                        title: q.name.clone(),
                        index: self.index % n + 1,
                        total: n,
                        ready: state.quest_ready(scene, q),
                        rows: q
                            .exercises
                            .iter()
                            .enumerate()
                            .map(|(i, e)| {
                                let (count, ready) = state.exercise_progress(scene, q, i);
                                format!(
                                    "{} {} · {}/{}",
                                    if ready {
                                        "Готово:"
                                    } else {
                                        "Нужно:"
                                    },
                                    exercise_text(scene, e),
                                    count.min(e.value),
                                    e.value
                                )
                            })
                            .collect(),
                        awards: format!(
                            "Награда: {}",
                            q.awards
                                .iter()
                                .map(|a| format!(
                                    "{} {}",
                                    a.count,
                                    if a.kind == "item" {
                                        item_name(scene, a.item_id)
                                    } else {
                                        resource_name(if a.kind == "experience" {
                                            "exp"
                                        } else {
                                            &a.kind
                                        })
                                        .into()
                                    }
                                ))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        primary_label: if state.quest_ready(scene, q) {
                            "Получить награду · Enter".into()
                        } else {
                            "Задание ещё не выполнено".into()
                        },
                    }
                } else {
                    PanelView {
                        section: "Задания".into(),
                        title: "Все доступные задания выполнены".into(),
                        ..base
                    }
                }
            }
            Kind::Land
            | Kind::Production
            | Kind::Research
            | Kind::Upgrades
            | Kind::ResourceProduction
            | Kind::ResourceSpeedup => base,
            Kind::Inventory => {
                let mut rows: Vec<_> = state
                    .inventory
                    .iter()
                    .filter(|(_, n)| **n > 0)
                    .map(|(id, n)| {
                        format!(
                            "{} · {n}",
                            id.parse()
                                .map(|id| item_name(scene, id))
                                .unwrap_or_else(|_| "Неизвестный предмет".into())
                        )
                    })
                    .collect();
                let pages = rows.len().div_ceil(5).max(1);
                let page = self.index % pages;
                rows = rows.into_iter().skip(page * 5).take(5).collect();
                if rows.is_empty() {
                    rows.push("В инвентаре пусто".into());
                }
                let capacity = scene
                    .economy
                    .as_ref()
                    .map(|c| {
                        format!(
                            "Нефть: {}/{} · Сталь: {}/{}",
                            state.resources["fuel"],
                            state.capacity(c, "fuel"),
                            state.resources["iron"],
                            state.capacity(c, "iron")
                        )
                    })
                    .unwrap_or_default();
                PanelView {
                    section: "Инвентарь".into(),
                    index: page + 1,
                    total: pages,
                    title: capacity,
                    rows,
                    awards: "Материалы используются в производстве и улучшениях".into(),
                    ..base
                }
            }
            Kind::Shop => {
                let items = state.shop_items(scene);
                let n = items.len();
                if let Some(item) = items.get(self.index % n.max(1)) {
                    let full =
                        item.id == 4000 && state.energy(scene, now) == state.max_energy(scene);
                    let affordable = item
                        .cost
                        .iter()
                        .all(|(k, n)| state.resources.get(k).copied().unwrap_or(0) >= *n);
                    PanelView {
                        section: "Ресурсы".into(),
                        title: item.name.clone(),
                        index: self.index % n + 1,
                        total: n,
                        ready: !full && affordable,
                        rows: item
                            .outputs
                            .iter()
                            .map(|a| {
                                if a.kind == "energy" {
                                    "Полностью восстановить энергию".into()
                                } else {
                                    format!("Добавить {} {}", a.count, resource_name(&a.kind))
                                }
                            })
                            .collect(),
                        awards: format!(
                            "Стоимость: {}",
                            item.cost
                                .iter()
                                .map(|(k, n)| format!("{n} {}", resource_name(k)))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        primary_label: if full {
                            "Энергия уже восстановлена".into()
                        } else if !affordable {
                            "Недостаточно ресурсов".into()
                        } else {
                            "Купить · Enter".into()
                        },
                    }
                } else {
                    PanelView {
                        section: "Ресурсы".into(),
                        title: "Нет доступных предложений".into(),
                        ..base
                    }
                }
            }
        })
    }
}
fn item_name(scene: &Scene, id: u32) -> String {
    scene
        .progression
        .as_ref()
        .and_then(|c| {
            c.special_items
                .get(&id)
                .map(|i| i.name.clone())
                .or_else(|| c.language.get(&format!("#item_{id}_name")).cloned())
        })
        .unwrap_or_else(|| format!("Предмет {id}"))
}
fn exercise_text(scene: &Scene, e: &Exercise) -> String {
    if !e.text.starts_with('#') && !e.text.is_empty() {
        return e.text.clone();
    }
    let verb = match e.kind.as_str() {
        "start_mission" => "Начать миссию",
        "complete_mission" => "Завершить миссию",
        "collect_entity" => "Построить или подготовить",
        "start_recipe" => "Запустить производство",
        "collect_research" => "Завершить исследование",
        "collect_upgrade" => "Завершить улучшение",
        "collect_pvp_playmoney" => "Собрать монеты в PvP",
        "collect_pvp_fuel" => "Собрать нефть в PvP",
        "collect_pvp_iron" => "Собрать сталь в PvP",
        "clear_junk" => "Очистить базу от препятствий",
        "buy_expansion" => "Расширить базу",
        _ => "Выполнить условие",
    };
    if e.item_id == 0 {
        return verb.into();
    }
    let name = scene.progression.as_ref().and_then(|c| {
        [
            format!("#MISSION_{}_NAME", e.item_id),
            format!("#mission_{}_name", e.item_id),
            format!("#item_{}_name", e.item_id),
            format!("#recipe_{}", e.item_id),
        ]
        .iter()
        .find_map(|k| c.language.get(k).cloned())
    });
    name.map_or_else(|| verb.into(), |name| format!("{verb}: {name}"))
}

impl Ui {
    fn guide_shop(&mut self, scene: &Scene, state: &State, id: u32) {
        if let Some(item) = scene.shop.as_ref().and_then(|s| s.items.get(&id)) {
            self.kind = Some(Kind::Construction);
            self.shop_tab = item.tab;
            let items = state.source_shop_items(scene, item.tab);
            let width = scene
                .interface
                .as_ref()
                .and_then(|c| c.symbols.get("shop"))
                .and_then(|root| {
                    Some(
                        root.frames.get("scroll_side_right")?[0]
                            + root.frames.get("scroll_side_right")?[2]
                            - root.frames.get("scroll_side_left")?[0],
                    )
                })
                .unwrap_or(465.);
            let index = items.iter().position(|it| it.id == id).unwrap_or(0);
            let scale = self.viewport_scale.max(1.);
            self.scroll = ((5. + 62.5 + index as f64 * 130. - width / 2.) * scale).clamp(
                0.,
                ((5. + items.len() as f64 * 130. - width) * scale).max(0.),
            );
        }
    }
    pub fn begin_quest_guide(
        &mut self,
        scene: &Scene,
        state: &State,
        camera: &mut Camera,
        quest: u32,
        index: usize,
    ) -> Result<(), String> {
        use warzone_app_client::quest_navigation::{self, Route};
        let mut route = quest_navigation::route(scene, state, quest, index)
            .ok_or("Нет доступной подсказки для этого задания")?;
        if !quest_navigation::visible(scene, state, quest, index) {
            return Err("Подсказка недоступна".into());
        }
        let requested = route.clone();
        let (resolved, mut uid) = quest_navigation::target(scene, state, route);
        route = resolved;
        if route == Route::Speedup {
            uid = state
                .entities
                .keys()
                .find(|u| {
                    state
                        .production_speedup_quote(
                            scene,
                            u,
                            self.now - scene.economy.as_ref().map_or(0, |c| c.time_shift),
                        )
                        .is_ok()
                })
                .cloned();
        }
        self.kind = None;
        self.selection = None;
        self.scroll = 0.;
        self.notice = None;
        self.recipe_access = None;
        self.lang_popup = false;
        self.mission_briefing = false;
        if let Some(e) = uid.as_ref().and_then(|u| state.entities.get(u)) {
            let p = scene.projection.project(
                (e["x"].as_i64().unwrap_or(0) - 16) as f64,
                (e["y"].as_i64().unwrap_or(0) - 16) as f64,
            );
            camera.x = p[0];
            camera.y = p[1] - camera.height as f64 * 0.1 / camera.zoom;
        } else if let Route::Building { quick: true, .. } = route {
            self.kind = Some(Kind::Construction);
            self.shop_tab = 2;
        }
        if let Route::Expansion(id) = &mut route {
            if *id == 0 {
                *id = state
                    .available_expansions(scene)
                    .into_iter()
                    .filter(|id| state.expansion_quote(scene, *id).is_ok())
                    .min_by_key(|id| scene.expansions.as_ref().unwrap().prices[id])
                    .ok_or("Нет доступного участка")?;
            }
            let [x, y] = warzone_app_client::world::expansion_position(*id as i32)?;
            let p = scene
                .projection
                .project((6 * x + 3) as f64, (6 * y + 3) as f64);
            camera.x = p[0];
            camera.y = p[1] - camera.height as f64 * 0.1 / camera.zoom;
        }
        self.guide = Some(Guide {
            started_revision: state.revision,
            menu_only: state
                .active_quests(scene)
                .iter()
                .find(|q| q.id == quest)
                .and_then(|q| q.exercises.get(index))
                .is_some_and(|e| {
                    e.item_id == 0
                        && matches!(
                            e.kind.as_str(),
                            "collect_fuel" | "collect_iron" | "collect_playmoney"
                        )
                })
                && uid
                    .as_deref()
                    .and_then(|u| state.entities.get(u))
                    .is_some_and(|e| e["productionType"] == 0 && e["state"] == 3),
            entry: Some(quest_navigation::entry(
                scene,
                state,
                &requested,
                uid.as_deref(),
                self.now,
            )),
            aligned_stage: String::new(),
            shop_category_done: false,
            stage: String::new(),
            shown_game: self.now,
            outside_taps: 0,
            outside_press: false,
            pointer_start: None,
            motion: None,
            since: self.animation_seconds,
            quest,
            index,
            route,
            uid,
        });
        Ok(())
    }
    pub fn begin_quest_guide_animated(
        &mut self,
        scene: &Scene,
        state: &State,
        camera: &mut Camera,
        quest: u32,
        index: usize,
    ) -> Result<(), String> {
        let original = camera.clone();
        self.begin_quest_guide(scene, state, camera, quest, index)?;
        let motion = warzone_app_client::quest_navigation::CameraMotion::new(
            &original,
            [camera.x, camera.y],
        );
        if motion.duration > 0. {
            camera.x = original.x;
            camera.y = original.y;
            self.guide.as_mut().unwrap().motion = Some(motion);
        }
        Ok(())
    }
    /// Keep rendering responsive while the original scroll owns map input.
    pub fn advance_quest_camera(&mut self, camera: &mut Camera) -> bool {
        let Some(g) = &mut self.guide else {
            return false;
        };
        let Some(m) = &g.motion else { return false };
        let (point, done) = m.sample(self.animation_seconds - g.since);
        camera.x = point[0];
        camera.y = point[1];
        if done {
            g.motion = None;
            g.since = self.animation_seconds;
            g.stage.clear();
            g.shown_game = self.now;
            g.outside_taps = 0;
        }
        true
    }
    pub fn quest_focus_active(&self) -> bool {
        self.guide.is_some()
    }
    // Source Step65 (normal building) and Step41 (harvester arrow) unlock
    // the world during placement. A preceding camera scroll still owns it.
    fn guide_placement(&self) -> bool {
        self.editing
            && self.guide.as_ref().is_some_and(|g| {
                g.motion.is_none()
                    && matches!(
                        g.route,
                        warzone_app_client::quest_navigation::Route::Building { .. }
                    )
            })
    }
    pub fn guide_map_locked(&self) -> bool {
        self.quest_focus_active() && !self.guide_placement()
    }
    pub fn quest_scrolling(&self) -> bool {
        self.guide.as_ref().is_some_and(|g| g.motion.is_some())
    }
    fn guide_cancel(
        &self,
        scene: &Scene,
        camera: &Camera,
        hole: [f64; 4],
    ) -> warzone_app_client::quest_navigation::CancelButton {
        use warzone_app_client::quest_navigation::CancelButton;
        let Some(g) = &self.guide else {
            return CancelButton::Hidden;
        };
        if g.quest == 990 {
            return CancelButton::Hidden;
        }
        let Some(button) = scene
            .interface
            .as_ref()
            .and_then(|c| c.symbols.get("tutorial_cancel"))
        else {
            return CancelButton::Hidden;
        };
        let s = warzone_app_client::hud::scale(camera);
        warzone_app_client::quest_navigation::cancel_button(
            hole,
            [button.size[0] * s, button.size[1] * s],
            [camera.width as f64, camera.height as f64],
            g.outside_taps,
            self.now - g.shown_game,
        )
    }
    pub fn update_guide_focus(
        &mut self,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) -> Option<String> {
        use warzone_app_client::quest_navigation::{CancelButton, Entry, Route};
        if self.guide.is_none() || self.quest_scrolling() {
            return None;
        }
        if matches!(self.kind, Some(Kind::ResourceProduction | Kind::Production))
            && self
                .guide
                .as_ref()
                .is_some_and(|g| g.menu_only && g.uid == self.selection)
        {
            self.guide = None;
            return None;
        }
        let mut selected = None;
        if let Some(entry) = self.guide.as_mut().and_then(|g| g.entry.take()) {
            match entry {
                Entry::ScrollOnly => {
                    self.guide = None;
                    return None;
                }
                Entry::OpenBusy => {
                    if let Some(uid) = self.guide.as_ref().and_then(|g| g.uid.clone()) {
                        if self.open_selected_window(scene, state, &uid, true) {
                            selected = Some(uid);
                        }
                    }
                }
                Entry::World => {}
            }
        }
        // ResearchDialog DisplayItem finishes when a prerequisite research
        // or factory is absent. It never forces its paid-access alternative.
        if self.kind == Some(Kind::Research)
            && self.guide.as_ref().is_some_and(|g| {
                if let Route::Research(id) = g.route {
                    scene
                        .research
                        .as_ref()
                        .and_then(|c| c.items.get(&id))
                        .is_some_and(|r| {
                            !state.research_ready(r.parent)
                                || r.entity != 0
                                    && !state.entities.values().any(|e| {
                                        e["type"] == r.entity
                                            && e["isDeleted"] != true
                                            && matches!(e["state"].as_i64(), Some(0 | 3))
                                    })
                        })
                } else {
                    false
                }
            })
        {
            self.guide = None;
            return selected;
        }
        // Source item/tab steps move the scroll view before creating a hole.
        // Use this window's actual native cell metrics, not world coordinates.
        let Some(g) = self.guide.as_ref() else {
            return selected;
        };
        if g.aligned_stage != g.stage {
            let wanted = match (&g.route, self.kind) {
                (Route::Building { id, .. }, Some(Kind::Construction | Kind::Shop))
                    if g.shop_category_done =>
                {
                    scene
                        .shop
                        .as_ref()
                        .and_then(|c| c.items.get(id))
                        .and_then(|item| {
                            let root = scene.interface.as_ref()?.symbols.get("shop")?;
                            let left = root.frames.get("scroll_side_left")?;
                            let right = root.frames.get("scroll_side_right")?;
                            let width = right[0] + right[2] - left[0];
                            state
                                .source_shop_items(scene, item.tab)
                                .iter()
                                .position(|it| it.id == *id)
                                .map(|index| {
                                    (5. + 62.5 + index as f64 * 130. - width / 2.)
                                        * warzone_app_client::hud::scale(camera)
                                })
                        })
                }
                (Route::Production { building, recipe }, Some(Kind::Production)) => scene
                    .workshop
                    .as_ref()
                    .and_then(|w| w.buildings.get(building))
                    .and_then(|ids| ids.iter().position(|id| id == recipe))
                    .map(|index| index as f64 * 127.5 * warzone_app_client::hud::scale(camera)),
                (Route::Upgrade(id), Some(Kind::Upgrades)) if self.upgrade_detail.is_none() => {
                    scene.upgrades.as_ref().and_then(|up| {
                        if let Some(unit) = self.upgrade_unit {
                            up.layers(unit)
                                .ok()?
                                .iter()
                                .position(|layer| layer.contains(id))
                                .map(|column| {
                                    column as f64 * 264. * warzone_app_client::hud::scale(camera)
                                })
                        } else {
                            let u = up.items.get(id)?;
                            let index = up.unit_order.iter().position(|unit| u.affects(*unit))?;
                            let template = scene
                                .interface
                                .as_ref()?
                                .symbols
                                .get("upgrades_bcm_gui_upgradeUnit_template")?;
                            Some(
                                index as f64
                                    * (template.size[0] + 5.)
                                    * warzone_app_client::hud::scale(camera),
                            )
                        }
                    })
                }
                _ => None,
            };
            if let Some(offset) = wanted {
                if let Some(view) = self.original_view(scene, state, camera) {
                    self.scroll = if matches!(self.kind, Some(Kind::Construction | Kind::Shop)) {
                        offset.max(0.)
                    } else {
                        offset.clamp(0., view.scroll_max)
                    };
                }
            }
            if let Some(g) = &mut self.guide {
                g.aligned_stage = g.stage.clone();
            }
        }
        let hole = self.original_view(scene, state, camera).and_then(|v| {
            v.draws
                .iter()
                .find(|d| d.node.kind == "tutorial_mask")
                .map(|d| d.node.rect)
        });
        if hole.is_some_and(|h| self.guide_cancel(scene, camera, h) == CancelButton::Abandon) {
            self.guide = None;
        }
        selected
    }
    fn guide_pointer(
        &mut self,
        event: &Event,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) -> bool {
        use warzone_app_client::{interface::Action as A, quest_navigation::focus_contains};
        if self.guide.is_none() {
            return false;
        }
        let placement = self.guide_placement();
        if matches!(
            event,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                ..
            }
        ) {
            self.guide = None;
            self.native_press = None;
            return !placement;
        }
        if placement {
            return false;
        }
        let Some(view) = self.original_view(scene, state, camera) else {
            return false;
        };
        let Some(hole) = view
            .draws
            .iter()
            .find(|d| d.node.kind == "tutorial_mask")
            .map(|d| d.node.rect)
        else {
            return false;
        };
        let g = self.guide.as_mut().unwrap();
        match *event {
            Event::MouseWheel { .. } => true,
            Event::KeyDown {
                keycode: Some(Keycode::Return | Keycode::Space),
                ..
            } => {
                // Native source buttons still use the desktop Enter bridge.
                // Admit that bridge only when Primary/Confirm is the actual
                // highlighted target; other shortcuts cannot bypass the mask.
                !view.hits.iter().any(|h| {
                    matches!(h.action, A::Primary | A::Confirm)
                        && focus_contains(
                            hole,
                            [h.rect[0] + h.rect[2] * 0.5, h.rect[1] + h.rect[3] * 0.5],
                        )
                })
            }
            Event::KeyDown {
                keycode: Some(Keycode::Delete),
                ..
            } => !view.hits.iter().any(|h| {
                h.action == A::Sell
                    && focus_contains(
                        hole,
                        [h.rect[0] + h.rect[2] * 0.5, h.rect[1] + h.rect[3] * 0.5],
                    )
            }),
            Event::KeyDown { .. } | Event::KeyUp { .. } => true,
            Event::MouseButtonDown {
                x,
                y,
                mouse_btn: MouseButton::Left,
                ..
            } => {
                if view.hit(x, y) == Some(A::TutorialCancel) {
                    return false;
                }
                let inside = focus_contains(hole, [x as f64, y as f64]);
                g.outside_press = !inside;
                g.pointer_start = inside.then_some([x, y]);
                !inside
            }
            Event::MouseButtonUp {
                x,
                y,
                mouse_btn: MouseButton::Left,
                ..
            } => {
                g.pointer_start = None;
                if std::mem::take(&mut g.outside_press) {
                    g.outside_taps = g.outside_taps.saturating_add(1);
                    return true;
                }
                view.hit(x, y) != Some(A::TutorialCancel)
                    && !focus_contains(hole, [x as f64, y as f64])
            }
            Event::MouseMotion { x, y, .. } => {
                if self.kind.is_some() {
                    return !focus_contains(hole, [x as f64, y as f64]);
                }
                g.pointer_start.is_none_or(|start| {
                    (x - start[0]).abs() + (y - start[1]).abs() > crate::input_events::slop(5)
                })
            }
            _ => false,
        }
    }
    fn guide_action(
        &self,
        view: &warzone_app_client::interface::View,
        scene: &Scene,
        state: &State,
    ) -> Option<warzone_app_client::interface::Action> {
        use warzone_app_client::{interface::Action as A, quest_navigation::Route};
        let g = self.guide.as_ref()?;
        let uid = g.uid.as_deref();
        let mut action = match &g.route {
            Route::Mission(id) => {
                if self.kind == Some(Kind::Missions) {
                    if self.mission_briefing {
                        Some(A::Primary)
                    } else {
                        scene
                            .combat
                            .as_ref()
                            .and_then(|c| c.campaign().position(|m| m.mission_id == *id))
                            .map(A::MissionSelect)
                    }
                } else {
                    Some(A::Missions)
                }
            }
            Route::Expansion(id) => Some(if self.kind == Some(Kind::Land) {
                A::LandCoins
            } else {
                A::LandOpen(*id)
            }),
            // Recovered Tutorials calls the old10040/start_btn. The retained
            // PvpProfileDialog now calls that same action visit_btn; bind the
            // current real hit instead of losing guidance after opening it.
            Route::Pvp => Some(if self.kind == Some(Kind::Pvp) {
                A::Primary
            } else {
                A::Pvp
            }),
            Route::Building { id, .. } if uid.is_none() => {
                if self.editing {
                    Some(A::Confirm)
                } else if matches!(self.kind, Some(Kind::Construction | Kind::Shop)) {
                    if !g.shop_category_done {
                        return scene
                            .shop
                            .as_ref()?
                            .items
                            .get(id)
                            .map(|i| A::ShopTab(i.tab));
                    }
                    Some(if view.hits.iter().any(|h| h.action == A::ShopBuy(*id)) {
                        A::ShopBuy(*id)
                    } else {
                        A::ShopUnlock(*id)
                    })
                } else {
                    Some(A::Construction)
                }
            }
            _ if self.kind == Some(Kind::ResourceSpeedup) => Some(A::ResourceSpeedup),
            Route::Production { recipe, .. } if self.kind.is_some() => {
                let ids = uid
                    .and_then(|u| state.entities.get(u))
                    .and_then(|e| e["type"].as_u64())
                    .and_then(|id| {
                        scene
                            .economy
                            .as_ref()
                            .and_then(|c| c.buildings.get(&(id as u32)).map(|b| b.recipes.clone()))
                            .or_else(|| {
                                scene
                                    .workshop
                                    .as_ref()
                                    .and_then(|c| c.buildings.get(&(id as u32)))
                                    .cloned()
                            })
                    });
                ids.and_then(|rs| rs.iter().position(|r| r == recipe))
                    .map(|index| {
                        if view
                            .hits
                            .iter()
                            .any(|h| h.action == A::RecipeAccess(*recipe))
                        {
                            A::RecipeAccess(*recipe)
                        } else {
                            A::Produce(index)
                        }
                    })
            }
            Route::Research(id) if self.kind == Some(Kind::Research) => {
                if self.research_detail {
                    Some(A::Primary)
                } else {
                    warzone_app_client::interface_windows::RESEARCH_ORDER
                        .iter()
                        .position(|r| r == id)
                        .map(|index| {
                            if view.hits.iter().any(|h| h.action == A::Choose(index)) {
                                A::Choose(index)
                            } else {
                                A::ResearchAccess(*id)
                            }
                        })
                }
            }
            Route::Upgrade(id) if self.kind == Some(Kind::Upgrades) => {
                if self.upgrade_detail.is_some() {
                    // Tutorials DisplayItem/UpgradeUnitDialog.getNeededItemPosition
                    // guides a missing ingredient before the actual start button.
                    scene
                        .upgrades
                        .as_ref()
                        .and_then(|c| c.items.get(id))
                        .and_then(|u| {
                            u.items
                                .iter()
                                .find(|(id, n)| {
                                    state.inventory.get(&id.to_string()).copied().unwrap_or(0) < **n
                                })
                                .map(|(id, _)| A::BuyMaterial(*id))
                        })
                        .or(Some(A::UpgradeStart(*id)))
                } else if self.upgrade_unit.is_some() {
                    Some(A::UpgradeOpen(*id))
                } else {
                    scene
                        .upgrades
                        .as_ref()
                        .and_then(|c| c.items.get(id))
                        .and_then(|u| {
                            scene
                                .upgrades
                                .as_ref()?
                                .unit_order
                                .iter()
                                .find(|id| u.affects(**id))
                                .copied()
                        })
                        .map(A::UpgradeUnit)
                }
            }
            _ => None,
        };
        if action.is_none()
            && self.selection.as_deref() == uid
            && uid.is_some()
            && !matches!(g.route, Route::Building { .. })
        {
            action = Some(if matches!(g.route, Route::ClearJunk) {
                A::Sell
            } else {
                A::Primary
            });
        }
        action
    }
    fn guide_overlay(
        &self,
        view: &mut warzone_app_client::interface::View,
        scene: &Scene,
        state: &State,
        camera: &Camera,
    ) {
        use warzone_app_client::quest_navigation::Route;
        let Some(g) = &self.guide else {
            return;
        };
        if g.motion.is_some() {
            return;
        } // source completion callback creates the hole
        let placing = self.guide_placement();
        if placing && !matches!(g.route, Route::Building { id: 1027, .. }) {
            return;
        }
        if self.notice.is_some()
            || self.recipe_access.is_some()
            || !state
                .active_quests(scene)
                .iter()
                .any(|q| q.id == g.quest && !state.exercise_progress(scene, q, g.index).1)
        {
            return;
        }
        let Some(symbol) = scene
            .interface
            .as_ref()
            .and_then(|c| c.symbols.get("tutorial_arrow"))
        else {
            return;
        };
        let uid = g.uid.as_deref();
        if uid.is_some_and(|u| state.gathering(u)) {
            return;
        }
        let action = self.guide_action(view, scene, state);
        let point = action
            .and_then(|a| view.hits.iter().find(|h| h.action == a))
            .map(|h| [h.rect[0] + h.rect[2] * 0.5, h.rect[1] + h.rect[3] * 0.5])
            .or_else(|| {
                if self.kind.is_none() {
                    uid.and_then(|u| {
                        warzone_app_client::quest_navigation::world_center(scene, state, camera, u)
                    })
                } else {
                    None
                }
            });
        if let Some(p) = point {
            let s = warzone_app_client::hud::scale(camera);
            let (phase, squash) =
                warzone_app_client::quest_navigation::tap_pose(self.animation_seconds - g.since);
            let amplitude = 20. * s;
            let mut arrow_y = p[1] - 7. * s - amplitude * (1. - phase);
            let speedup = matches!(
                action,
                Some(warzone_app_client::interface::Action::ResourceSpeedup)
            );
            let mission_marker = matches!(g.route, Route::Mission(_))
                && self.kind == Some(Kind::Missions)
                && !self.mission_briefing;

            if let Some(template) = symbol
                .nodes
                .first()
                .filter(|_| !placing && !self.mission_briefing)
            {
                let hole = if mission_marker {
                    // Tutorials.miniTutorialShowStep 0x13bf99: new mission
                    // ring80x104, lifted25pt; the source suppresses its arrow.
                    Some(([100. * s, 104. * s], -25. * s))
                } else if speedup {
                    action
                        .and_then(|a| view.hits.iter().find(|h| h.action == a))
                        .map(|h| ([h.rect[2] + 10. * s, h.rect[3] + 10. * s], 0.))
                } else if matches!(
                    action,
                    Some(
                        warzone_app_client::interface::Action::LandCoins
                            | warzone_app_client::interface::Action::LandCash
                    )
                ) {
                    action
                        .and_then(|a| view.hits.iter().find(|h| h.action == a))
                        .map(|h| ([h.rect[2] + 10. * s, h.rect[3] + 10. * s], 0.))
                } else if matches!(action, Some(warzone_app_client::interface::Action::Sell)) {
                    Some(([65. * s, 50. * s], 0.))
                } else if action.is_none() {
                    let active =
                        uid.and_then(|u| state.entities.get(u))
                            .is_some_and(|e| match g.route {
                                Route::Production { recipe, .. } => {
                                    e["productionType"].as_u64() == Some(recipe as u64)
                                }
                                _ => false,
                            });
                    warzone_app_client::quest_navigation::world_hole(&g.route, active).map(
                        |(size, pivot)| ([size[0] * camera.zoom, size[1] * camera.zoom], pivot * s),
                    )
                } else {
                    let size = if matches!(g.route, Route::Pvp) && self.kind == Some(Kind::Pvp) {
                        [180., 80.]
                    } else if matches!(g.route, Route::Mission(_))
                        && self.kind == Some(Kind::Missions)
                        && !self.mission_briefing
                    {
                        [100., 100.]
                    } else {
                        [100., 50.]
                    };
                    Some(([size[0] * s, size[1] * s], 0.))
                };
                if let Some((size, pivot)) = hole {
                    warzone_app_client::quest_navigation::draw_source_focus(
                        view,
                        template,
                        [
                            p[0] - size[0] / 2.,
                            p[1] + pivot - size[1] / 2.,
                            size[0],
                            size[1],
                        ],
                        scene
                            .interface
                            .as_ref()
                            .and_then(|c| c.symbols.get("tutorial_ring")),
                    );
                    if action.is_none() {
                        arrow_y = p[1] + pivot - size[1] / 2. + 15. * s - amplitude * (1. - phase);
                    } else if matches!(g.route, Route::Pvp) && self.kind == Some(Kind::Pvp) {
                        arrow_y -= 8. * s;
                    }
                }
            }
            if let Some(r) = action
                .and_then(|a| view.hits.iter().find(|h| h.action == a))
                .map(|h| h.rect)
            {
                if self.mission_briefing
                    || matches!(
                        action,
                        Some(
                            warzone_app_client::interface::Action::Primary
                                | warzone_app_client::interface::Action::Produce(_)
                        )
                    )
                {
                    arrow_y = r[1] + 15. * s - amplitude * (1. - phase);
                }
            }
            if !speedup && !mission_marker {
                let begin = view.draws.len();
                warzone_app_client::quest_navigation::draw_arrow(
                    view,
                    symbol,
                    [p[0] / s, arrow_y / s],
                    s,
                );
                for d in &mut view.draws[begin..] {
                    let bottom = d.node.rect[1] + d.node.rect[3];
                    d.node.rect[3] *= 1. - 0.25 * squash;
                    d.node.rect[1] = bottom - d.node.rect[3];
                }
            }
            if let Some(hole) = view
                .draws
                .iter()
                .find(|d| d.node.kind == "tutorial_mask")
                .map(|d| d.node.rect)
            {
                if let (
                    Some(button),
                    warzone_app_client::quest_navigation::CancelButton::Visible(rect),
                ) = (
                    scene
                        .interface
                        .as_ref()
                        .and_then(|c| c.symbols.get("tutorial_cancel")),
                    self.guide_cancel(scene, camera, hole),
                ) {
                    warzone_app_client::quest_navigation::draw_cancel(view, button, rect, s);
                }
            }
        }
    }
}

#[cfg(test)]
mod pointer_view_checks {
    use super::*;
    #[test]
    fn league_swipe_settles_to_a_page_without_inertial_drift() {
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/pack");
        let scene = Scene::load(&pack).unwrap();
        let seed =
            warzone_app_client::save::digest(&std::fs::read(pack.join("scene.json")).unwrap());
        let state = State::seed(&scene, seed).unwrap();
        let camera = Camera {
            x: 0.,
            y: 0.,
            zoom: 1.,
            width: 1920,
            height: 1080,
        };
        let width = warzone_app_client::interface_leagues::page_width(&scene, &camera).unwrap();
        let mut ui = Ui {
            kind: Some(Kind::LeaguesHelp),
            scroll_kind: Some(Kind::LeaguesHelp),
            scroll: width * 0.72,
            scroll_velocity: 1200.,
            scroll_drag: Some((1000, 0.)),
            ..Ui::default()
        };
        let before = ui.scroll;
        ui.event(
            &Event::MouseButtonUp {
                timestamp: 0,
                window_id: 1,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x: 600,
                y: 700,
            },
            &camera,
            &scene,
            &state,
        );
        assert_eq!(ui.scroll, before, "Release must not jump to the page");
        assert_eq!(ui.scroll_page_target, Some(width));
        ui.animation_seconds = 1. / 60.;
        ui.update_time_ms(&scene, &state, 0);
        assert!(ui.scroll > before && ui.scroll < width);
        assert!(ui.original_view(&scene, &state, &camera).unwrap().scrolling);
        for frame in 2..=120 {
            ui.animation_seconds = frame as f64 / 60.;
            ui.update_time_ms(&scene, &state, 0);
        }
        assert_eq!(ui.scroll, width);
        assert_eq!(ui.scroll_velocity, 0.);
        assert!(!ui.original_view(&scene, &state, &camera).unwrap().scrolling);
        assert!(
            ui.original_view(&scene, &state, &camera)
                .unwrap()
                .draws
                .iter()
                .any(|d| d.node.parents.iter().any(|p| p == "source_pause_world"))
        );
    }
    #[test]
    fn lazy_marker_hits_preserve_the_drawn_hud_and_map_action_order() {
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/pack");
        let scene = Scene::load(&pack).unwrap();
        let seed =
            warzone_app_client::save::digest(&std::fs::read(pack.join("scene.json")).unwrap());
        let state = State::seed(&scene, seed).unwrap();
        let camera = Camera {
            x: scene.camera[0],
            y: scene.camera[1],
            zoom: 1.,
            width: 1920,
            height: 1080,
        };
        let ui = Ui::default();
        let full = ui.original_view(&scene, &state, &camera).unwrap();
        assert!(full.hits.len() > 5);
        for hit in &full.hits {
            let (x, y) = (
                (hit.rect[0] + hit.rect[2] / 2.) as i32,
                (hit.rect[1] + hit.rect[3] / 2.) as i32,
            );
            let event = Event::MouseButtonDown {
                timestamp: 0,
                window_id: 1,
                which: 0,
                mouse_btn: MouseButton::Left,
                clicks: 1,
                x,
                y,
            };
            let fast = ui.event_view(&event, &scene, &state, &camera).unwrap();
            assert_eq!(fast.hit(x, y), full.hit(x, y));
        }
    }
}
