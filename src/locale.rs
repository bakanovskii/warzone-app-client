//! Original langID (0 English, 1 Russian, 2 Chinese). Presentation projection
//! changes only; archived entities, receipts, timers and gameplay RNG are intact.
use crate::{Scene, production::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Binding {
    pub path: String,
    pub key: String,
    pub fallback: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub catalogs: BTreeMap<u32, BTreeMap<String, String>>,
    pub bindings: Vec<Binding>,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.catalogs.len() != 3
            || !(0..=2).all(|id| self.catalogs.contains_key(&id))
            || self.catalogs.values().any(|c| {
                c.is_empty()
                    || c.len() > 20000
                    || c.iter().any(|(k, v)| k.len() > 512 || v.len() > 32768)
            })
            || self.bindings.len() > 10000
            || self.bindings.iter().any(|b| {
                let p: Vec<_> = b.path.split('/').collect();
                let valid = matches!(
                    p.as_slice(),
                    ["", "entities", _, "name"]
                        | ["", "world", "catalog", _, "name"]
                        | ["", "shop", "items", _, "name"]
                        | ["", "research", "items", _, "name"]
                        | ["", "upgrades", "items", _, "name"]
                        | ["", "workshop", "recipes", _, "name"]
                        | ["", "combat", "missions", _, "title"]
                        | ["", "progression", "special_items", _, "name"]
                        | ["", "progression", "quests", _, "name"]
                        | [
                            "",
                            "progression",
                            "quests",
                            _,
                            "exercises",
                            _,
                            "text" | "hint"
                        ]
                );
                !valid || b.path.len() > 256 || b.key.len() > 512 || b.fallback.len() > 32768
            })
        {
            return Err("Invalid original localization projection".into());
        }
        Ok(())
    }
}
impl State {
    pub fn language_id(&self) -> u32 {
        self.systems
            .as_ref()
            .filter(|s| s.preferences.contains_key("langID"))
            .map(|s| s.integer("langID"))
            .filter(|id| (0..=2).contains(id))
            .unwrap_or(1) as u32
    }
    pub fn set_language(&self, scene: &Scene, id: u32) -> Result<(Self, String), String> {
        if scene
            .localization
            .as_ref()
            .is_none_or(|c| !c.catalogs.contains_key(&id))
        {
            return Err("Language is unavailable in this asset package".into());
        }
        let mut next = self.clone();
        next.systems
            .as_mut()
            .ok_or("Missing preferences")?
            .preferences
            .insert("langID".into(), id.into());
        next.validate(scene)?;
        Ok((next, String::new()))
    }
}
impl Scene {
    pub fn select_language(&mut self, id: u32) -> Result<(), String> {
        let Some(config) = &self.localization else {
            return Ok(());
        };
        config.validate()?;
        let mut strings = config
            .catalogs
            .get(&id)
            .ok_or("Unknown source language")?
            .clone();
        for (k, v) in strings.clone() {
            strings.entry(k.to_ascii_lowercase()).or_insert(v);
        }
        let mut raw = serde_json::to_value(&*self).map_err(|e| e.to_string())?;
        for b in &config.bindings {
            let target = raw
                .pointer_mut(&b.path)
                .filter(|v| v.is_string())
                .ok_or("Localization binding is not a text field")?;
            *target = strings
                .get(&b.key)
                .or_else(|| strings.get(&b.key.to_ascii_lowercase()))
                .unwrap_or(&b.fallback)
                .clone()
                .into();
        }
        raw["progression"]["language"] =
            serde_json::to_value(strings).map_err(|e| e.to_string())?;
        // Ambient clips are transient Arc references, deliberately absent from
        // serialized scene/save data. Text localization must retain them too,
        // including the initial language application at game startup.
        let mut translated = serde_json::from_value(raw).map_err(|e| e.to_string())?;
        let mut clips = Vec::new();
        crate::animation::for_each_track(self, |t| clips.push(t.ambient.clone()));
        let mut clips = clips.into_iter();
        let mut missing = false;
        crate::animation::for_each_track(&mut translated, |t| match clips.next() {
            Some(clip) => t.ambient = clip,
            None => missing = true,
        });
        if missing || clips.next().is_some() {
            return Err("Localization changed animation track structure".into());
        }
        *self = translated;
        // Scene::load sorts instances by map position. Bind their names by
        // type identity, never by the original JSON's array indices.
        if let Some(world) = &self.world {
            for entity in &mut self.entities {
                if let Some(def) = world.catalog.get(&entity.type_id) {
                    entity.name = def.name.clone();
                }
            }
        }
        Ok(())
    }
}
