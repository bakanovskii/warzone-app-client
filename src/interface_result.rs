//! Original BattleEndDialog mission outcomes, award strip and three loss lists.
use crate::{
    Camera, Scene,
    interface::{self, Action, View},
    production::State,
};
use std::collections::BTreeMap;
pub fn rows(state: &State) -> [BTreeMap<u32, usize>; 3] {
    let mut rows: [BTreeMap<u32, usize>; 3] = Default::default();
    if let Some(m) = &state.mission {
        for u in &m.battle.units {
            if u.hp > 0 {
                continue;
            }
            // Unit.enemy is the unit AI flag, not a building allegiance flag.
            // Mission structures (e.g.1500) use fraction"buildings" with it false.
            let row =
                if u.kind == "building" && u.fraction != "player" && (1000..2000).contains(&u.id) {
                    Some(0)
                } else if u.enemy && (2000..3000).contains(&u.id) {
                    Some(1)
                } else if u.fraction == "player" && (2000..3000).contains(&u.id) {
                    Some(2)
                } else {
                    None
                };
            if let Some(r) = row {
                *rows[r].entry(u.id).or_default() += 1;
            }
        }
    }
    rows
}
pub fn icon(scene: &Scene, id: u32) -> Option<&'static str> {
    Some(match id {
        2000 | 2001 | 2101..=2107 => "unit1",
        2002 | 2108 => "unit2",
        2005 => "scout1",
        2029 | 2109 => "scout2",
        2016 | 2017 | 2110 | 2111 | 2115 => "sniper",
        2007 | 2114 | 2117 => "jeep",
        2009 | 2116 => "bmp",
        2011 => "tank",
        2015 => "artillery",
        2112 => "zombie_01",
        2113 => "zombie_02",
        2018 => "unit0",
        1000 => "headquaters",
        1003 | 1034 | 1500 => "barracks",
        1006 | 1502 => "factory",
        1005 => "training",
        1007 => "fuel",
        1010 => "mine_iron",
        1021 | 1036 => "turrel",
        1016 | 1035 => "warehouse",
        1027 => "harvester",
        1019 => "academy",
        1020 => "research",
        1999 => "heliport",
        1038 | 1501 => "laboratory",
        _ => return model_icon(scene, id),
    })
}
fn model_icon(scene: &Scene, id: u32) -> Option<&'static str> {
    // Mission aliases share the original model family, not the player ID.
    if let Some(model) = scene
        .combat
        .as_ref()
        .and_then(|c| c.defs.get(&id))
        .filter(|d| d.group == "units")
        .map(|d| d.model.as_str())
    {
        for (suffix, name) in [
            ("newtrooper_00", "unit0"),
            ("newtrooper_01", "unit1"),
            ("newtrooper_02", "unit2"),
            ("newtrooper_03", "unit2"),
            ("scout_01", "scout1"),
            ("scout_02", "scout2"),
            ("newsniper_01", "sniper"),
            ("newsniper_02", "sniper"),
            ("newsniper_03", "sniper"),
            ("jeep_01", "jeep"),
            ("bmp_01", "bmp"),
            ("bmp_02", "bmp"),
            ("tank", "tank"),
            ("artillery", "artillery"),
            ("zombie_01", "zombie_01"),
            ("zombie_02", "zombie_02"),
        ] {
            if model.ends_with(suffix) {
                return Some(name);
            }
        }
        // The original summary has no separate portrait for Skалов.
        if model.ends_with("skalov") {
            return Some("unit0");
        }
    }
    None
}
pub fn add(
    v: &mut View,
    scene: &Scene,
    state: &State,
    camera: &Camera,
    scroll: [f64; 3],
) -> Option<()> {
    let session = state.mission.as_ref()?;
    if session
        .battle
        .tutorial
        .as_ref()
        .is_some_and(|t| t.index >= 26)
    {
        return None;
    }
    let result = session.battle.result.as_deref()?;
    let c = scene.interface.as_ref()?;
    let root = c.symbols.get("battle_result")?;
    let s = crate::hud::scale(camera);
    let o = [
        (camera.width as f64 / s - root.size[0]) / 2.,
        (camera.height as f64 / s - root.size[1]) / 2. + 2.,
    ];
    v.hits.clear();
    v.blocks.clear();
    v.modal = true;
    v.popup_shade(root.nodes.first()?, camera);
    let win = result == "win";
    let mission = &scene.combat.as_ref()?.missions[&session.battle.mission];
    let pvp_awards: Vec<_> = session
        .battle
        .pvp
        .as_ref()
        .map(|p| p.stolen())
        .into_iter()
        .flat_map(|counts| crate::pvp::RESOURCES.into_iter().zip(counts))
        .filter(|(_, n)| *n > 0)
        .map(|(kind, n)| crate::systems::Award {
            kind: kind.into(),
            count: n.to_string(),
            item_id: 0,
        })
        .collect();
    let awards = if session.battle.pvp.is_some() {
        pvp_awards.as_slice()
    } else if win && session.reward_eligible {
        mission.awards.as_slice()
    } else {
        &[]
    };
    let kinds = [
        ("playMoney", "play_money_txt", "playMoney"),
        ("experience", "exp_txt", "exp"),
        ("fuel", "oil_txt", "fuel"),
        ("iron", "iron_txt", "iron"),
        ("realMoney", "realMoney_txt", "realMoney"),
        ("energy", "max_txt", "energyMax"),
    ];
    let active: Vec<_> = kinds
        .iter()
        .filter_map(|(kind, text, group)| {
            awards
                .iter()
                .find(|a| a.kind == *kind)
                .map(|a| (*text, *group, a))
        })
        .collect();
    let visible = |n: &crate::interface::Node| {
        if ["fadeLeft", "fadeRight", "listBackground"].contains(&n.id.as_str()) {
            return false;
        }
        if n.id.starts_with("honor_") || n.parents.iter().any(|p| p.starts_with("honor_")) {
            return session.battle.pvp.is_some()
                && (if win {
                    n.id.starts_with("honor_win")
                        || n.parents.iter().any(|p| p.starts_with("honor_win"))
                } else {
                    n.id.starts_with("honor_loose")
                        || n.parents.iter().any(|p| p.starts_with("honor_loose"))
                });
        }
        if n.parents
            .iter()
            .any(|p| p == if win { "logoDefeat" } else { "logoWin" })
            || n.id == if win { "imageDef" } else { "imageWin" }
        {
            return false;
        }
        if n.id == "award_txt" && active.is_empty() {
            return false;
        }
        for (_, _, group) in kinds {
            if n.parents.iter().any(|p| p == group) && !active.iter().any(|(_, g, _)| *g == group) {
                return false;
            }
        }
        true
    };
    let start = v.draws.len();
    v.add(
        root,
        o,
        s,
        visible,
        |n| {
            Some(match n.id.as_str() {
                "win_text" => interface::lang(scene, "#victory", "").to_uppercase(),
                "defeat_text" => interface::lang(scene, "#defeat", "").to_uppercase(),
                "award_txt" => interface::lang(scene, "#quest_reward", "").into(),
                "honor_win_txt" => [0, 33, 67, 100]
                    [session.battle.pvp_score().map_or(0, |s| s.stars) as usize]
                    .to_string(),
                "honor_loose_txt" => "100".into(),
                "enemies_txt" => interface::lang(scene, "#enemies_destroyed", "").into(),
                "buildings_txt" => interface::lang(scene, "#buildings_destroyed", "").into(),
                "lost_txt" => interface::lang(scene, "#units_lost", "").into(),
                "ok_txt" | "btn_txt" => interface::lang(scene, "#OK", "").into(),
                id => active
                    .iter()
                    .find(|(text, _, _)| *text == id)
                    .map(|(_, _, a)| {
                        if a.count == "max" {
                            "MAX".into()
                        } else {
                            format!("+{}", a.count)
                        }
                    })
                    .unwrap_or_else(|| n.text.clone()),
            })
        },
        None,
    );
    for d in &mut v.draws[start..] {
        if let Some((i, (_, group, _))) = active
            .iter()
            .enumerate()
            .find(|(_, (_, g, _))| d.node.parents.iter().any(|p| p == *g))
        {
            let source = root.frames[*group];
            let target = 182. - active.len() as f64 * 15. + i as f64 * 30. + 15.;
            d.node.rect[0] += (target - source[0] - source[2] / 2.) * s;
        }
    }
    let data = rows(state);
    for (row, counts) in data.iter().enumerate() {
        let template = &c.symbols[if row == 2 {
            "battle_result_lostItems"
        } else {
            "battle_result_enemyItems"
        }];
        let y = 151. + row as f64 * 49.;
        let clip = [(o[0] + 92.) * s, (o[1] + y - 26.) * s, 238. * s, 49. * s];
        v.scroll_max = v
            .scroll_max
            .max((counts.len() as f64 * 44. + 30. - 238.).max(0.) * s);
        for (i, (id, count)) in counts.iter().enumerate() {
            let chosen = icon(scene, *id);
            let fallback =
                row == 2 && chosen.is_some_and(|key| !template.nodes.iter().any(|n| n.id == key));
            let template = if fallback {
                &c.symbols["battle_result_enemyItems"]
            } else {
                template
            };
            let start = v.draws.len();
            v.add(
                template,
                [o[0] + 92. + i as f64 * 44. - scroll[row] / s, o[1] + y],
                s,
                |n| n.id == "bg" || n.id == "count_txt" || Some(n.id.as_str()) == chosen,
                |n| {
                    if n.id == "count_txt" {
                        Some(count.to_string())
                    } else {
                        None
                    }
                },
                Some(clip),
            );
            if fallback {
                for d in &mut v.draws[start..] {
                    d.grayscale = true;
                }
            }
        }
    }
    let fade_start = v.draws.len();
    v.add(
        root,
        o,
        s,
        |n| n.id == "fadeLeft" || n.id == "fadeRight",
        |_| None,
        None,
    );
    // BattleEndDialog.initWithMode:0x12571d hides listBackground after
    // deriving three scroll frames, then reattaches fades to the dialog.
    // Their authored positions are placeholders, not their runtime frames.
    let list = root.frames["listBackground"];
    for d in &mut v.draws[fade_start..] {
        let width = d.node.rect[2] / s;
        d.node.rect[0] = (o[0]
            + if d.node.id == "fadeLeft" {
                list[0] - 13.
            } else {
                list[0] + list[2] - width + 10.
            })
            * s;
        d.node.rect[1] = (o[1] + list[1] + 20.) * s;
    }
    v.button(root, "close_btn", o, s, Action::Close);
    v.button(root, "ok_btn", o, s, Action::Primary);
    Some(())
}
