//! Retained original interface geometry shared by rendering and hit testing.
//! Layout nodes come from the application, not native debug panel coordinates.
use crate::{Camera, Scene, production::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<[u32; 4]>,
    pub id: String,
    pub group: String,
    pub kind: String,
    pub rect: [f64; 4],
    #[serde(default)]
    pub rotation: f64,
    pub texture: String,
    pub text: String,
    pub font_family: String,
    pub font_size: f64,
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip_x: bool,
    pub align: String,
    pub line_height: f64,
    pub multiline: bool,
    pub color: u32,
    #[serde(default)]
    pub parents: Vec<String>,
    #[serde(default)]
    pub letter_spacing: f64,
    #[serde(default)]
    pub shadow: Option<TextEffect>,
    #[serde(default)]
    pub glow: Option<TextEffect>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextEffect {
    pub color: u32,
    pub alpha: f64,
    pub strength: f64,
    #[serde(rename = "blurX")]
    pub blur_x: f64,
    #[serde(rename = "blurY")]
    pub blur_y: f64,
    #[serde(default, rename = "offsetX")]
    pub offset_x: f64,
    #[serde(default, rename = "offsetY")]
    pub offset_y: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Symbol {
    pub size: [f64; 2],
    pub nodes: Vec<Node>,
    pub buttons: BTreeMap<String, [f64; 4]>,
    #[serde(default)]
    pub slots: BTreeMap<String, [f64; 4]>,
    #[serde(default)]
    pub frames: BTreeMap<String, [f64; 4]>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub symbols: BTreeMap<String, Symbol>,
    pub item_icons: BTreeMap<u32, Symbol>,
    #[serde(default)]
    pub unit_thumbs: BTreeMap<u32, String>,
    #[serde(default)]
    pub building_thumbs: BTreeMap<u32, String>,
    #[serde(default)]
    pub resource_icons: BTreeMap<u32, String>,
    #[serde(default)]
    pub work_icons: BTreeMap<u32, Symbol>,
    #[serde(default)]
    pub quest_icons: BTreeMap<u32, String>,
    #[serde(default)]
    pub quest_symbols: BTreeMap<String, Symbol>,
    #[serde(default)]
    pub exercise_icons: BTreeMap<String, Symbol>,
}
impl Config {
    pub fn validate(&self, scene: &Scene) -> Result<(), String> {
        for name in [
            "main",
            "settings",
            "settings_button",
            "inventory",
            "inventory_item_template",
            "building",
            "confirm",
            "rotate_new",
        ] {
            if !self.symbols.contains_key(name) {
                return Err(format!("Missing original interface symbol {name}"));
            }
        }
        for symbol in self
            .symbols
            .values()
            .chain(self.item_icons.values())
            .chain(self.quest_symbols.values())
            .chain(self.exercise_icons.values())
            .chain(self.work_icons.values())
        {
            if symbol.nodes.len() > 2048 || !symbol.size.iter().all(|x| x.is_finite() && *x > 0.) {
                return Err("Invalid original UI symbol bounds".into());
            }
            for node in &symbol.nodes {
                if !node.rect.iter().all(|x| x.is_finite())
                    || !node.rotation.is_finite()
                    || node.rotation.abs() > 360.
                    || node.rect[2] <= 0.
                    || node.rect[3] <= 0.
                    || node.color > 0xffffff
                {
                    return Err("Invalid original UI node geometry".into());
                }
                if !node.letter_spacing.is_finite()
                    || node.letter_spacing.abs() > 20.
                    || node.parents.len() > 32
                    || [node.shadow.as_ref(), node.glow.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|e| {
                            e.color > 0xffffff
                                || ![
                                    e.alpha, e.strength, e.blur_x, e.blur_y, e.offset_x, e.offset_y,
                                ]
                                .iter()
                                .all(|x| x.is_finite())
                                || !(0. ..=1.).contains(&e.alpha)
                                || !(0. ..=64.).contains(&e.strength)
                                || e.blur_x.abs() > 16.
                                || e.blur_y.abs() > 16.
                                || e.offset_x.abs() > 16.
                                || e.offset_y.abs() > 16.
                        })
                {
                    return Err("Invalid original UI text effect".into());
                }
                if node.kind == "image" {
                    if node.source.is_some_and(|r| {
                        scene.textures.get(&node.texture).is_none_or(|t| {
                            r[2] == 0
                                || r[3] == 0
                                || r[0].checked_add(r[2]).is_none_or(|n| n > t.width)
                                || r[1].checked_add(r[3]).is_none_or(|n| n > t.height)
                        })
                    }) {
                        return Err("Invalid original UI crop".into());
                    }
                    if !scene.textures.contains_key(&node.texture) {
                        return Err("Missing original UI image".into());
                    }
                } else if node.kind != "text"
                    || !node.font_size.is_finite()
                    || !(1. ..=80.).contains(&node.font_size)
                {
                    return Err("Invalid original UI text style".into());
                }
            }
        }
        if self
            .unit_thumbs
            .values()
            .chain(self.resource_icons.values())
            .chain(self.building_thumbs.values())
            .any(|t| !scene.textures.contains_key(t))
        {
            return Err("Missing original unit thumbnail".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Inventory,
    Settings,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Achievements,
    ClaimAchievement(u32, usize),
    Energy,
    Bank,
    BankClaim(u32),
    EnergyBuy,
    EnergyWait,
    BonusPickup(u32),
    FallingPickup(u64, u32, usize),
    ShowQuest(u32, usize),
    TutorialCancel,
    LandOpen(u32),
    LandCoins,
    LandCash,
    LandTopup,
    Inventory,
    Settings,
    Construction,
    Missions,
    MissionSelect(usize),
    Pvp,
    PvpHistory,
    PvpLeagues,
    LeagueSelect(usize),
    LeaguesHelp,
    Close,
    Music,
    Sound,
    Move,
    Rotate,
    Sell,
    Primary,
    Confirm,
    Cancel,
    Language,
    English,
    Russian,
    Chinese,
    Facebook,
    GameCenter,
    Friends,
    Forum,
    Support,
    Previous,
    Next,
    Choose(usize),
    Produce(usize),
    ShopTab(u32),
    ShopBuy(u32),
    ShopUnlock(u32),
    UpgradeUnit(u32),
    UpgradeOpen(u32),
    UpgradeInfo(u32),
    UpgradeStart(u32),
    UpgradeBuy(u32),
    BuyMaterial(u32),
    MaterialInfo(u32),
    BuyResearch(u32),
    ResearchAccess(u32),
    RecipeAccess(u32),
    ResourceSpeedup,
    ObjectPrimary(usize),
    Dismiss,
    Quest(usize),
    ClaimQuest(u32),
}
#[derive(Clone, Serialize)]
pub struct Draw {
    pub node: Node,
    pub clip: Option<[f64; 4]>,
    pub progress: f64,
    pub opacity: u8,
    pub grayscale: bool,
}
#[derive(Clone)]
pub struct Hit {
    pub action: Action,
    pub rect: [f64; 4],
}
#[derive(Clone, Default)]
pub struct View {
    pub deployment_preview: Option<([[f64; 2]; 4], bool)>,
    /// Additional clear ellipses in the original UIViewWithHole overlay.
    pub extra_tutorial_holes: Vec<[f64; 4]>,
    /// Ephemeral SceneMain sale selection; never stored in the game profile.
    pub sale_tint_uid: Option<String>,
    pub retained_unit_health: bool,
    pub retained_battle_selection: bool,
    pub battle_route: Vec<[f64; 2]>,
    pub world_boundary: Vec<[[f64; 2]; 2]>,
    pub base_selection: Option<[[f64; 2]; 4]>,
    pub battle_attack_area: Vec<[f64; 2]>,
    pub draws: Vec<Draw>,
    pub hits: Vec<Hit>,
    pub blocks: Vec<[f64; 4]>,
    pub modal: bool,
    pub scroll_max: f64,
}
/// Fonts.getTopMarginForFontName:size:isBold:isItalic:0xd25f5.
/// Exact source family comparison; bold/italic flags do not affect the margin.
/// Retained original960x640 source capture:65px column centers. The35-point
/// bitmap has transparent margins; its visible artwork is53px, not its70px quad.
pub fn sidebar_origin(
    camera: &Camera,
    _size: [f64; 2],
    column: u32,
    settings: bool,
) -> ([f64; 2], f64) {
    let s = crate::hud::scale(camera);
    let w = camera.width as f64 / s;
    let h = camera.height as f64 / s;
    (
        [
            w - 31.5 - column as f64 * 32.5,
            h - if settings { 126. } else { 94.5 },
        ],
        s,
    )
}
pub fn source_font_top_margin(family: &str, points: f64) -> f64 {
    if family != "Candara" {
        0.
    } else if points >= 15. {
        3.5
    } else if points >= 11. {
        2.5
    } else if points >= 9. {
        1.5
    } else {
        0.
    }
}
/// InventoryDialog.moveUp resets the root frame, replacing its exported pivot.
/// Child coordinates were flattened before that move, so subtract the pivot.
/// The fallback is the pinned1.11.2inventory.json root; future packs retain it.
pub fn inventory_origin(symbol: &Symbol, camera: &Camera) -> [f64; 2] {
    let p = symbol.slots.get("__source_root_frame").copied().unwrap_or([
        -237.5,
        -10.5,
        symbol.size[0],
        symbol.size[1],
    ]);
    [
        -p[0],
        camera.height as f64 / crate::hud::scale(camera) - symbol.size[1] - p[1],
    ]
}
/// Extend phone-wide panels without stretching their type, icons or cards.
/// Keep centred content centred, and keep right-side controls at the right.
pub fn wide_panel(symbol: &Symbol, camera: &Camera, right: &[&str]) -> Symbol {
    let mut result = symbol.clone();
    let extra = (camera.width as f64 / crate::hud::scale(camera) - 568.).max(0.);
    result.size[0] += extra;
    for node in &mut result.nodes {
        if node.kind == "image" && node.parents.is_empty() && node.rect[2] >= 450. {
            node.rect[2] += extra;
        } else {
            node.rect[0] += if node.parents.iter().any(|p| right.contains(&p.as_str()))
                || right.contains(&node.id.as_str())
            {
                extra
            } else {
                extra / 2.
            };
        }
    }
    for table in [&mut result.frames, &mut result.slots, &mut result.buttons] {
        for (id, rect) in table {
            rect[0] += if right.contains(&id.as_str()) {
                extra
            } else {
                extra / 2.
            };
        }
    }
    result
}
pub fn contains(r: [f64; 4], p: [f64; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}
impl View {
    pub(crate) fn popup_shade(&mut self, template: &Node, camera: &Camera) {
        // Dialogs.createDialog:useGrayBackground:useCenter: loads 0x3f19999a
        // (0.6f), verified in Thumb at0x117db0. This layer precedes the popup.
        let mut node = template.clone();
        node.id = "source_popup_shade".into();
        node.kind = "shade".into();
        node.texture.clear();
        node.text.clear();
        node.parents.clear();
        node.color = 0;
        node.rect = [0., 0., camera.width as f64, camera.height as f64];
        self.draws.push(Draw {
            node,
            clip: None,
            progress: 1.,
            opacity: 153,
            grayscale: false,
        });
    }
    pub fn hit(&self, x: i32, y: i32) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|h| contains(h.rect, [x as f64, y as f64]))
            .map(|h| h.action)
    }
    pub fn blocks(&self, x: i32, y: i32) -> bool {
        self.modal
            || self
                .blocks
                .iter()
                .any(|r| contains(*r, [x as f64, y as f64]))
    }
    pub fn add(
        &mut self,
        symbol: &Symbol,
        origin: [f64; 2],
        scale: f64,
        visible: impl Fn(&Node) -> bool,
        text: impl Fn(&Node) -> Option<String>,
        clip: Option<[f64; 4]>,
    ) {
        for n in &symbol.nodes {
            if !visible(n) {
                continue;
            }
            let mut node = n.clone();
            node.rect = [
                (n.rect[0] + origin[0]) * scale,
                (n.rect[1] + origin[1]) * scale,
                n.rect[2] * scale,
                n.rect[3] * scale,
            ];
            let margin = source_font_top_margin(&n.font_family, n.font_size) * scale;
            if margin != 0. {
                node.parents.push(format!("source_top_margin_{margin}"));
            }
            if node.kind == "text" {
                node.parents.push(format!("source_label_scale_{scale}"));
            }
            node.font_size *= scale;
            node.line_height *= scale;
            node.letter_spacing *= scale;
            for e in [&mut node.shadow, &mut node.glow].into_iter().flatten() {
                e.blur_x *= scale;
                e.blur_y *= scale;
                e.offset_x *= scale;
                e.offset_y *= scale;
            }
            if let Some(value) = text(n) {
                node.text = value;
            }
            self.draws.push(Draw {
                node,
                clip,
                progress: 1.,
                opacity: 255,
                grayscale: false,
            });
        }
    }
    pub(crate) fn button(
        &mut self,
        symbol: &Symbol,
        id: &str,
        origin: [f64; 2],
        s: f64,
        action: Action,
    ) {
        if let Some(r) = symbol.buttons.get(id) {
            let rect = [
                (r[0] + origin[0]) * s,
                (r[1] + origin[1]) * s,
                r[2] * s,
                r[3] * s,
            ];
            self.hits.push(Hit { action, rect });
            self.blocks.push(rect);
        }
    }
}
/// Original localization lookup shared by retained views and desktop modal flow.
pub fn lang<'a>(scene: &'a Scene, key: &str, fallback: &'a str) -> &'a str {
    scene
        .progression
        .as_ref()
        // Localizations.getLocalizedStr:defaultValue:param: (0x23d35)
        // lowercases the lookup key; source callers also pass #BUY_FOR/#OK.
        .and_then(|p| {
            p.language
                .get(key)
                .or_else(|| p.language.get(&key.to_lowercase()))
                // Existing packs retain XML spelling; the original dictionary
                // normalizes it while loading. Keep those packs compatible too.
                .or_else(|| {
                    p.language
                        .iter()
                        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                        .map(|(_, value)| value)
                })
        })
        .map(String::as_str)
        .unwrap_or(fallback)
}
pub fn build(
    scene: &Scene,
    state: &State,
    camera: &Camera,
    screen: Option<Screen>,
    selected: Option<&str>,
    editing: bool,
    scroll: f64,
    lang_popup: bool,
) -> Option<View> {
    let c = scene.interface.as_ref()?;
    let mut v = View::default();
    let s = crate::hud::scale(camera);
    let w = camera.width as f64 / s;
    let h = camera.height as f64 / s;
    let sym = &c.symbols["main"];
    let origin = [w - sym.size[0] - 5., h - 45.];
    v.add(
        sym,
        origin,
        s,
        |n| n.group != "attacksNumber" && n.kind != "text",
        |_| None,
        None,
    );
    let mut bottom_range = 0..v.draws.len();
    for (id, a) in [
        ("shop_btn", Action::Construction),
        ("inventory_btn", Action::Inventory),
        ("missions_btn", Action::Missions),
        ("battle_btn", Action::Pvp),
    ] {
        v.button(sym, id, origin, s, a);
    }
    let main_draw_end = v.draws.len();
    let main_hit_end = v.hits.len();
    let settings = &c.symbols["settings_button"];
    let (origin, button_scale) = sidebar_origin(camera, settings.size, 0, true);
    v.add(settings, origin, button_scale, |_| true, |_| None, None);
    v.button(settings, "settings", origin, button_scale, Action::Settings);
    crate::interface_achievements::button(&mut v, scene, state, camera);
    crate::interface_leagues::button(&mut v, scene, camera);
    if let Some(friends) = c.symbols.get("friends_button") {
        let (o, z) = sidebar_origin(camera, friends.size, 0, false);
        v.add(friends, o, z, |_| true, |_| None, None);
        v.button(friends, "friends", o, z, Action::Friends);
    }
    crate::interface_energy::hud(&mut v, scene, camera);
    if let Some(uid) = selected {
        let id = state
            .entities
            .get(uid)
            .and_then(|e| e["type"].as_u64())
            .unwrap_or(0) as u32;
        let selected = &c.symbols["building"];
        let o = [w - selected.size[0] - 5., h - 45.];
        let production = match id {
            1005 => "troops",
            1006 => "jeep",
            1019 | 1020 => "research_btn",
            1027 => "harvester",
            1016 | 1035 => "inventory",
            1038 => "laboratory",
            1007..=1009 => "oil",
            1010..=1012 => "iron",
            _ => "",
        };
        v.draws.drain(..main_draw_end);
        v.hits.drain(..main_hit_end);
        bottom_range = v.draws.len()..v.draws.len();
        v.add(
            selected,
            o,
            s,
            |n| {
                [
                    if production.is_empty() {
                        "bkg3"
                    } else {
                        "bkg4"
                    },
                    "move_btn",
                    "rotate_btn",
                    "destroy_btn",
                    production,
                ]
                .contains(&n.group.as_str())
            },
            |_| None,
            None,
        );
        for (id, a) in [
            ("move_btn", Action::Move),
            ("rotate_btn", Action::Rotate),
            ("destroy_btn", Action::Sell),
            (
                production,
                if production == "inventory" {
                    Action::Inventory
                } else {
                    Action::Primary
                },
            ),
        ] {
            let enabled = match a {
                Action::Primary => !state.gathering(uid),
                Action::Move => state.can_edit_world(scene, uid, false).is_ok(),
                Action::Rotate => state.can_edit_world(scene, uid, true).is_ok(),
                Action::Sell => {
                    state.sale_quote(scene, uid).is_ok()
                        || state.can_clear_obstacle(scene, uid).is_ok()
                }
                _ => true,
            };
            if enabled {
                v.button(selected, id, o, s, a);
            } else {
                for draw in &mut v.draws {
                    if draw.node.group == id {
                        draw.opacity = 128;
                    }
                }
                if let Some(r) = selected.buttons.get(id) {
                    v.blocks
                        .push([(r[0] + o[0]) * s, (r[1] + o[1]) * s, r[2] * s, r[3] * s]);
                }
            }
        }
    }
    if editing {
        if selected.is_none() {
            v.draws.drain(..main_draw_end);
            v.hits.drain(..main_hit_end);
            bottom_range = v.draws.len()..v.draws.len();
        }
        let confirm = &c.symbols["confirm"];
        // CHUDBottom.showConfirmDialog0x115291: confirm goes LEFT OF the
        // current item strip (currentWidth=bkg3/bkg4), preserving that strip.
        let width = if selected.is_some() {
            if v.draws.iter().any(|d| d.node.id == "bkg4") {
                211.
            } else {
                160.
            }
        } else {
            0.
        };
        let origin = [
            w - confirm.size[0] - if selected.is_some() { 10. + width } else { 5. },
            h - 45.,
        ];
        v.add(confirm, origin, s, |_| true, |_| None, None);
        v.button(confirm, "ok_btn", origin, s, Action::Confirm);
        v.button(confirm, "cancel_btn", origin, s, Action::Cancel);
        if selected.is_none() {
            let rotate = &c.symbols["rotate_new"];
            let o = [origin[0] - rotate.size[0] - 10., origin[1]];
            v.add(rotate, o, s, |_| true, |_| None, None);
            v.button(rotate, "rotate_new_item_btn", o, s, Action::Rotate);
        }
    }
    if editing || selected.is_some() {
        bottom_range.end = v.draws.len();
    }
    crate::interface_windows::quest_badges(&mut v, scene, state, camera, true);
    match screen {
        None => {}
        Some(Screen::Settings) => {
            v.draws.clear();
            v.blocks.clear();
            v.modal = true;
            v.hits.clear();
            crate::interface_windows::quest_badges(&mut v, scene, state, camera, false);
            bottom_range = v.draws.len()..v.draws.len();
            let responsive = wide_panel(&c.symbols["settings"], camera, &["cancel_btn"]);
            let sym = &responsive;
            let o = [(w - sym.size[0]) * 0.5, h - sym.size[1] + 4.];
            let language = state.language_id();
            v.add(
                sym,
                o,
                s,
                |n| match n.group.as_str() {
                    "musicon_btn" => state.audio_enabled("playMusic"),
                    "musicoff_btn" => !state.audio_enabled("playMusic"),
                    "soundon_btn" => state.audio_enabled("playSound"),
                    "soundoff_btn" => !state.audio_enabled("playSound"),
                    "langRu_btn" => language == 1,
                    "langEn_btn" => language == 0,
                    "langChi_btn" => language == 2,
                    "popupEn_btn" => lang_popup && language != 0,
                    "popupRu_btn" => lang_popup && language != 1,
                    "popupChi_btn" => false,
                    "Language_txt" if lang_popup => false,
                    "fbConnect_txt" => false,
                    _ => true,
                },
                |n| {
                    Some(
                        match n.id.as_str() {
                            "header_txt" => lang(scene, "#settings", "Настройки"),
                            "music_txt" => lang(scene, "#settings_music", "Музыка"),
                            "sound_txt" => lang(scene, "#settings_sound", "Звуки"),
                            "Language_txt" => lang(scene, "#settings_lang", "Язык"),
                            "FaceBook_txt" => "Facebook",
                            "Forum_txt" => lang(scene, "#settings_forum", "Форум"),
                            "GameCentre_txt" => "Game Center",
                            "bugReport_txt" => {
                                lang(scene, "#settings_bugreport", "Связаться с нами")
                            }
                            "playerid_txt" => {
                                return Some(format!(
                                    "{}: {}",
                                    lang(scene, "#player_id", "ID игрока"),
                                    state.local_player_id()
                                ));
                            }
                            "fbDisconnect_txt" => lang(scene, "#facebook_disconnected", "Отключён"),
                            _ => &n.text,
                        }
                        .into(),
                    )
                },
                None,
            );
            for (id, a) in [
                ("cancel_btn", Action::Close),
                ("musicon_btn", Action::Music),
                ("soundon_btn", Action::Sound),
                ("langRu_btn", Action::Language),
                ("langEn_btn", Action::Language),
                ("langChi_btn", Action::Language),
                ("facebook_btn", Action::Facebook),
                ("gameCentre_btn", Action::GameCenter),
                ("forum_btn", Action::Forum),
                ("bugReport_btn", Action::Support),
            ] {
                if id != "langRu_btn" && id != "langEn_btn" && id != "langChi_btn"
                    || (id == "langRu_btn" && language == 1)
                    || (id == "langEn_btn" && language == 0)
                    || (id == "langChi_btn" && language == 2)
                {
                    v.button(sym, id, o, s, a);
                }
            }
            if lang_popup {
                // CHUDBottom.onButtonLanguage0x115e..: hide current language,
                // position alternatives after the flag, and explicitly hide
                // Chinese last. Chinese imported profiles remain supported.
                let flag = sym.buttons.get("langEn_btn")?;
                let mut x = flag[0] + flag[2] + 5.;
                for (id, a, show) in [
                    ("popupEn_btn", Action::English, language != 0),
                    ("popupRu_btn", Action::Russian, language != 1),
                ] {
                    if !show {
                        continue;
                    }
                    let r = sym.buttons.get(id)?;
                    let dx = (x - r[0]) * s;
                    for d in &mut v.draws {
                        if d.node.parents.iter().any(|p| p == id) {
                            d.node.rect[0] += dx;
                        }
                    }
                    let count = v.hits.len();
                    v.button(sym, id, o, s, a);
                    if let Some(hit) = v.hits.get_mut(count) {
                        hit.rect[0] += dx;
                    }
                    x += r[2] + 5.;
                }
            }
        }
        Some(Screen::Inventory) => {
            v.draws.clear();
            v.blocks.clear();
            v.modal = true;
            v.hits.clear();
            crate::interface_windows::quest_badges(&mut v, scene, state, camera, false);
            bottom_range = v.draws.len()..v.draws.len();
            let sym = &c.symbols["inventory"];
            let o = inventory_origin(sym, camera);
            let extra = (w - 480.).max(0.);
            let body_start = v.draws.len();
            let items: Vec<_> = state.inventory.iter().filter(|(_, n)| **n > 0).collect();
            let capacity = |key: &str| scene.economy.as_ref().map_or(0, |c| state.capacity(c, key));
            v.add(
                sym,
                o,
                s,
                |n| {
                    !n.group.ends_with("_template")
                        && (n.id != "empty_text" || items.is_empty())
                        && n.group != "icon_place"
                },
                |n| match n.id.as_str() {
                    "Header_txt" => Some(lang(scene, "#inventory", "Инвентарь").into()),
                    "empty_text" => {
                        Some(lang(scene, "#inventory_empty", "В инвентаре пусто.").into())
                    }
                    "fuel_value_txt" => {
                        Some(format!("{}/{}", state.resources["fuel"], capacity("fuel")))
                    }
                    "metal_value_txt" => {
                        Some(format!("{}/{}", state.resources["iron"], capacity("iron")))
                    }
                    _ => None,
                },
                None,
            );
            // Original phone-wide branch moves centered groups44points and
            // the right-hand close button88points for the568point viewport.
            for d in &mut v.draws[body_start..] {
                if d.node.kind == "image" && d.node.parents.is_empty() && d.node.rect[2] >= 450. * s
                {
                    d.node.rect[0] = 0.;
                    d.node.rect[2] = camera.width as f64;
                } else if d.node.parents.iter().any(|p| {
                    ["empty_text", "Header_txt", "fuel_bar", "metal_bar"].contains(&p.as_str())
                }) {
                    d.node.rect[0] += extra * 0.5 * s;
                } else if d.node.parents.iter().any(|p| p == "close_btn") {
                    d.node.rect[0] += extra * s;
                }
            }
            for d in &mut v.draws {
                let key = match d.node.id.as_str() {
                    "fuel_progress" => Some("fuel"),
                    "metal_progress" => Some("iron"),
                    _ => None,
                };
                if let Some(key) = key {
                    let cap = capacity(key);
                    d.progress = if cap > 0 {
                        (state.resources[key] as f64 / cap as f64).clamp(0., 1.)
                    } else {
                        0.
                    };
                }
            }
            v.button(sym, "close_btn", o, s, Action::Close);
            if let Some(hit) = v.hits.last_mut() {
                hit.rect[0] += extra * s;
            }
            let template = &c.symbols["inventory_item_template"];
            let width = template.size[0];
            let spacing = 3.;
            let total = (width + spacing) * items.len() as f64 - spacing;
            v.scroll_max = (total - w + 24.).max(0.) * s;
            let scroll = scroll / s;
            let start = if total < w {
                (w - total) * 0.5
            } else {
                12. - scroll
            };
            let clip = [0., (h - 127.) * s, camera.width as f64, 127. * s];
            for (i, (id, count)) in items.into_iter().enumerate() {
                let id = id.parse::<u32>().ok()?;
                // Source scroll height127; center the101point item view with
                // roundf((127-height)/2), then compensate its-.55root pivot.
                let origin = [
                    start + i as f64 * (width + spacing) + 37.5,
                    h - 127. + ((127. - template.size[1]) * 0.5).round() + 0.55,
                ];
                v.add(
                    template,
                    origin,
                    s,
                    |_| true,
                    |n| match n.id.as_str() {
                        "number_txt" => Some(count.to_string()),
                        "name_txt" => Some(scene.item_name(id, "").into()),
                        _ => None,
                    },
                    Some(clip),
                );
                // InventoryDialog.updateItems calls generateIconLarge (type31),
                // and keeps the authored circular icon_place background.
                if let Some(icon) = c
                    .symbols
                    .get(&format!("speedup_item_{id}"))
                    .or_else(|| c.item_icons.get(&id))
                {
                    v.add(
                        icon,
                        [origin[0], origin[1] + 57.55],
                        s,
                        |_| true,
                        |_| None,
                        Some(clip),
                    );
                }
            }
        }
    }
    if screen.is_some() {
        bottom_range.end = v.draws.len();
    }
    for draw in &mut v.draws[bottom_range] {
        draw.node.parents.push("source_bottom_slide".into());
    }
    Some(v)
}
