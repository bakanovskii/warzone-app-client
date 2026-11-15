//! Original interface assets. Layout of the native dialogs remains independent.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Font {
    pub path: String,
    pub sha256: String,
    pub family: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub fonts: BTreeMap<String, Font>,
    pub panel: String,
    pub button: String,
    pub close: String,
}
impl Config {
    pub fn validate(&self, scene: &crate::Scene) -> Result<(), String> {
        if ["body", "button", "title", "level"]
            .iter()
            .any(|k| !self.fonts.contains_key(*k))
            || [&self.panel, &self.button, &self.close]
                .iter()
                .any(|k| !scene.textures.contains_key(*k))
        {
            return Err("Missing original interface assets".into());
        }
        Ok(())
    }
}
