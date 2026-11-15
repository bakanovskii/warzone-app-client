//! Original offline PvP profile and NPC arena widgets. No fabricated opponents.
use crate::{
    Camera, Scene,
    interface::{self, Action, View},
    production::State,
};

pub fn profile(v: &mut View, scene: &Scene, state: &State, camera: &Camera) -> Option<()> {
    let responsive = interface::wide_panel(
        scene.interface.as_ref()?.symbols.get("pvp_profile")?,
        camera,
        &["close_btn"],
    );
    let root = &responsive;
    let scale = crate::hud::scale(camera);
    // PvpProfileDialog.init: phone frame is bottom aligned, +4 logical points.
    let o = [
        (camera.width as f64 / scale).min(568.) / 2. - 240.,
        camera.height as f64 / scale - root.size[1] + 4.,
    ];
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    let integer = |key: &str| state.systems.as_ref().map_or(0, |p| p.integer(key));
    let league = state
        .systems
        .as_ref()
        .and_then(|p| p.preferences.get("league"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("wooden");
    let honor = integer("honor");
    let limit = integer("honorLvL");
    let remaining = integer("remainingMatches");
    let lang = |key: &str| interface::lang(scene, key, "").replace("\\n", "\n");
    let start = v.draws.len();
    v.add(
        root,
        o,
        scale,
        |n| {
            let id = n.id.as_str();
            if ["attacks_txt", "attacksNumber", "league_name_en"].contains(&id)
                || n.parents.iter().any(|p| p == "attacksNumber")
            {
                return false;
            }
            if id.ends_with("_trophy") {
                return id == format!("{league}_trophy");
            }
            if id == "in_gold_league" {
                return league == "gold" && remaining <= 0;
            }
            if id == "games_ramaining" {
                return remaining > 0;
            }
            if ["combat_points", "golden_league"].contains(&id) {
                return league != "gold" && remaining <= 0;
            }
            true
        },
        |n| {
            Some(match n.id.as_str() {
                "multiplayer" => lang("#profile_title"),
                "name_text" => state
                    .systems
                    .as_ref()
                    .and_then(|p| p.preferences.get("playerName"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("Local player")
                    .into(),
                "attack_text" => lang("#profile_button_defence_history"),
                "top_text" => lang("#profile_button_leagues"),
                "Find_match" => lang("#profile_button_find"),
                "multiplayer_match" => lang("#profile_match_cost"),
                "play_money_text" => {
                    crate::pvp::match_price(state.resources.get("level").copied().unwrap_or(0))
                        .unwrap_or(0)
                        .to_string()
                }
                "honor_number" => honor.to_string(),
                "attacks_won" => lang("#profile_attacks_won"),
                "attacks_lost" => lang("#profile_attacks_lost"),
                "defences_won" => lang("#profile_defenses_won"),
                "defences_lost" => lang("#profile_defenses_lost"),
                "attacks_won_score" => integer("pvpActiveWins").to_string(),
                "attacks_lost_score" => integer("pvpActiveLost").to_string(),
                "defences_won_score" => integer("pvpPassiveWins").to_string(),
                "defences_lost_score" => integer("pvpPassiveLosts").to_string(),
                "league_name_ru" => lang(&format!("#league_{league}")),
                "league_text" => lang(if league == "wooden" {
                    "#league_type_no"
                } else {
                    "#league_type"
                }),
                "combat_points" => crate::interface_resources::number_text(
                    &lang("#profile_need_honor"),
                    limit - honor,
                ),
                "golden_league" => lang(&format!(
                    "#profile_need_for_{}_league",
                    match league {
                        "bronze" => "silver",
                        _ => "gold",
                    }
                )),
                "in_gold_league" => lang("#profile_you_are_in_best_league"),
                "games_ramaining" => crate::interface_resources::number_text(
                    &lang("#pvp_qualifiers_left"),
                    remaining,
                ),
                "time_to_end" => lang("#profile_time_expired"),
                _ => String::new(),
            })
        },
        None,
    );
    for d in &mut v.draws[start..] {
        if d.node.id == "bar_progress" {
            d.progress = if limit > 0 {
                (honor as f64 / limit as f64).clamp(0., 1.)
            } else {
                0.
            };
        }
    }
    v.button(root, "close_btn", o, scale, Action::Close);
    v.button(root, "visit_btn", o, scale, Action::Primary);
    v.button(root, "attack_history", o, scale, Action::PvpHistory);
    v.button(root, "top_players", o, scale, Action::PvpLeagues);
    Some(())
}
pub fn arena(v: &mut View, scene: &Scene, state: &State, camera: &Camera) -> Option<()> {
    let b = &state.mission.as_ref()?.battle;
    let p = b.pvp.as_ref()?;
    let c = scene.interface.as_ref()?;
    let scale = crate::hud::scale(camera);
    let width = camera.width as f64 / scale;
    let portrait = c.symbols.get("pvp_enemy")?;
    if b.phase == "placement" {
        v.add(
            portrait,
            [4., 60.],
            scale,
            |n| !n.id.ends_with("_trophy"),
            |n| {
                Some(match n.id.as_str() {
                    "level_txt" => "5".into(),
                    "user_name" => interface::lang(scene, "#pvptutorial_user", "").into(),
                    _ => String::new(),
                })
            },
            None,
        );
    }
    let loot = c.symbols.get("pvp_state")?;
    let remaining = p.remaining();
    v.add(
        loot,
        [width - 76., 32.],
        scale,
        |_| true,
        |n| {
            Some(match n.id.as_str() {
                "loot_txt" => {
                    interface::lang(scene, "#pvp_available_loot", "").replace("\\n", "\n")
                }
                "def_txt" => interface::lang(scene, "#pvp_ui_defeat", "").into(),
                "play_money_txt" => remaining[0].to_string(),
                "fuel_txt" => remaining[1].to_string(),
                "iron_txt" => remaining[2].to_string(),
                "honor_plus" => "+100".into(),
                "honor_minus" => "-100".into(),
                _ => String::new(),
            })
        },
        None,
    );
    if b.phase == "battle" {
        if let Some(stars) = c.symbols.get("pvp_stars") {
            let score = b.pvp_score()?;
            v.add(
                stars,
                [6., 60.],
                scale,
                |n| {
                    n.id.strip_prefix("start")
                        .and_then(|s| s.parse::<u32>().ok())
                        .is_none_or(|i| i <= score.stars)
                },
                |n| {
                    Some(match n.id.as_str() {
                        "damage_txt" => format!(
                            "{}%",
                            if score.total == 0 {
                                0
                            } else {
                                (score.destroyed as f32 * 100. / score.total as f32) as i32
                            }
                        ),
                        "damage_caption_txt" => interface::lang(scene, "#pvp_damage", "").into(),
                        _ => String::new(),
                    })
                },
                None,
            );
        }
    }
    Some(())
}
pub fn timer(v: &mut View, scene: &Scene, state: &State, camera: &Camera, now: i64) -> Option<()> {
    let session = state.mission.as_ref()?;
    if session.battle.pvp.is_none()
        || session.battle.phase != "battle"
        || session.battle.result.is_some()
    {
        return None;
    }
    let root = scene.interface.as_ref()?.symbols.get("pvp_time")?;
    let scale = crate::hud::scale(camera);
    let seconds = (2700 - (now - session.opened).max(0)).max(0);
    v.add(
        root,
        [(camera.width as f64 / scale - root.size[0]) / 2., 32.],
        scale,
        |_| true,
        |n| {
            Some(match n.id.as_str() {
                "time_txt" => format!("{}:{:02}", seconds / 60, seconds % 60),
                "time_caption_txt" => interface::lang(scene, "#pvp_battle_endsin", "").into(),
                _ => String::new(),
            })
        },
        None,
    );
    Some(())
}
