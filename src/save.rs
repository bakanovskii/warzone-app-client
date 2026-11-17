//! One locked native profile. Atomic replacement commits reward AND timer reset.
//! Recover damaged writes from validated local backups, never an implicit new game.
use crate::{
    Scene,
    production::{Command, State},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(not(target_os = "emscripten"))]
use std::io::Read;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    sha256: String,
    state: State,
}
#[derive(Serialize)]
struct WrittenEnvelope<'a> {
    format: &'static str,
    sha256: String,
    state: &'a State,
}
fn encode(state: &State) -> Result<Vec<u8>, String> {
    let sha256 = digest(&serde_json::to_vec(state).map_err(|e| e.to_string())?);
    serde_json::to_vec_pretty(&WrittenEnvelope {
        format: save_format(state),
        sha256,
        state,
    })
    .map_err(|e| e.to_string())
}
fn save_format(state: &State) -> &'static str {
    if state.actions.iter().any(|a| a.kind == "bank_free") {
        "warzone-desktop-save-v8"
    } else if state.actions.iter().any(|a| a.at_ms.is_some()) {
        "warzone-desktop-save-v7"
    } else if needs_achievement_reader(state) {
        "warzone-desktop-save-v6"
    } else if needs_pvp_reader(state) {
        "warzone-desktop-save-v5"
    } else if needs_quest_story_reader(state) {
        "warzone-desktop-save-v4"
    } else if needs_movement_bonus_reader(state) {
        "warzone-desktop-save-v3"
    } else if needs_async_reader(state) {
        "warzone-desktop-save-v2"
    } else {
        "warzone-desktop-save-v1"
    }
}
// Older executables ignore unknown preference keys. Explicitly raise the outer
// envelope version once a profile contains delayed gather/flight semantics, so
// those executables reject it rather than collecting the same timer immediately.
// State serialization stays exact; pre-gather ordinary profiles remain byte-
// compatible with v1. Both published v1 asynchronous and new v2 profiles load.
fn needs_async_reader(state: &State) -> bool {
    state
        .mission
        .as_ref()
        .is_some_and(|m| m.battle.camera_request.is_some() || m.battle.tutorial.is_some())
        || state.actions.iter().any(|a| {
            matches!(
                a.kind.as_str(),
                "gather_resource"
                    | "gather_material"
                    | "gather_research"
                    | "gather_upgrade"
                    | "gather_upgrade_flight"
            )
        })
}
fn needs_achievement_reader(state: &State) -> bool {
    state.actions.iter().any(|a| a.kind == "achievement_claim")
}
fn needs_pvp_reader(state: &State) -> bool {
    state
        .mission
        .as_ref()
        .is_some_and(|m| m.battle.pvp.is_some())
        || state
            .actions
            .iter()
            .any(|a| a.recipe == 5990 && a.kind == "mission_open" || a.kind == "pvp_loot")
}
fn needs_quest_story_reader(state: &State) -> bool {
    state
        .mission
        .as_ref()
        .and_then(|m| m.battle.story.as_ref())
        .and_then(|s| s.message.as_ref())
        .is_some_and(|m| m.canonical_phrases)
        || state
            .actions
            .iter()
            .any(|a| a.kind.starts_with("quest_story_") || a.kind == "tutorial_base_finish")
}
fn needs_movement_bonus_reader(state: &State) -> bool {
    state.actions.iter().any(|a| a.kind == "mission_move_bonus")
}
fn supported_format(format: &str) -> bool {
    matches!(
        format,
        "warzone-desktop-save-v1"
            | "warzone-desktop-save-v2"
            | "warzone-desktop-save-v3"
            | "warzone-desktop-save-v4"
            | "warzone-desktop-save-v5"
            | "warzone-desktop-save-v6"
            | "warzone-desktop-save-v7"
            | "warzone-desktop-save-v8"
    )
}
// Recover interrupted/corrupt bytes, but do not roll back a valid save from a
// newer client, a different pack, or an invalid game-state/schema migration.
fn damaged_write(bytes: &[u8]) -> bool {
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return true,
    };
    if !json["format"].as_str().is_some_and(supported_format) {
        return false;
    }
    match serde_json::from_value::<Envelope>(json) {
        Ok(data) => {
            serde_json::to_vec(&data.state).is_ok_and(|state| data.sha256 != digest(&state))
        }
        Err(_) => true,
    }
}
fn decode(bytes: &[u8], scene: &Scene, seed: &str) -> Result<(State, bool), String> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Native save exceeds supported size".into());
    }
    let mut data: Envelope =
        serde_json::from_slice(bytes).map_err(|e| format!("Damaged native save: {e}"))?;
    if !supported_format(&data.format) {
        return Err("Unsupported save version; refusing to replace it with an older backup".into());
    }
    if data.sha256 != digest(&serde_json::to_vec(&data.state).map_err(|e| e.to_string())?) {
        return Err("Native save checksum mismatch; refusing to reset it".into());
    }
    if data.state.seed_sha256 != seed
        && !scene.provenance["compatible_native_seeds"]
            .as_array()
            .is_some_and(|ids| {
                ids.iter()
                    .any(|v| v.as_str() == Some(data.state.seed_sha256.as_str()))
            })
    {
        return Err(
            "This profile belongs to another asset/seed pack; use a different --profile".into(),
        );
    }
    // No repair is attempted until checksum and exact seed ancestry pass.
    // These audited early packs only supported production and did not export
    // owned land. Their original imported entity identities still must match.
    let mut legacy_land = false;
    if data.format == "warzone-desktop-save-v1"
        && scene.provenance["legacy_pre_land_seeds"]
            .as_array()
            .is_some_and(|seeds| {
                seeds
                    .iter()
                    .any(|s| s.as_str() == Some(data.state.seed_sha256.as_str()))
            })
        && data
            .state
            .actions
            .iter()
            .all(|a| matches!(a.kind.as_str(), "start" | "collect"))
        && data.state.entities.len() == scene.entities.len()
        && scene
            .entities
            .iter()
            .all(|e| data.state.entities.contains_key(&e.uid))
    {
        if let Some(systems) = &mut data.state.systems {
            if !systems.preferences.contains_key("expansionsMap") {
                let land = scene
                    .progression
                    .as_ref()
                    .and_then(|p| p.seed_preferences.get("expansionsMap"))
                    .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
                    .ok_or("Missing original land for audited legacy migration")?;
                systems
                    .preferences
                    .insert("expansionsMap".into(), land.clone());
                legacy_land = true;
            }
        } else {
            legacy_land = data.state.initialize_systems(scene)?;
        }
    }
    data.state.validate(scene)?;
    // Preserve a higher supported envelope; migrate only when the required
    // reader rises. v7 protects exact callback semantics from older executables.
    let migrate = data.format.as_bytes().last() < save_format(&data.state).as_bytes().last();
    Ok((data.state, migrate || legacy_land))
}
/// Validate an explicit browser import before replacing any existing profile.
pub fn validate_import(bytes: &[u8], scene: &Scene, seed: &str) -> Result<(), String> {
    decode(bytes, scene, seed).map(|_| ())
}
fn read_regular(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => Err(format!(
            "Expected a regular native save: {}",
            path.display()
        )),
        Ok(meta) if meta.len() > 16 * 1024 * 1024 => {
            Err("Native save exceeds supported size".into())
        }
        Ok(_) => fs::read(path).map(Some).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
fn temporary_path(directory: &Path, name: &str) -> Result<PathBuf, String> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // PIDs can repeat after a reboot (and always repeat in Emscripten). Stale
    // crash temporaries must not block a future save or recovery transaction.
    Ok(directory.join(format!(
        ".{name}.{}.{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )))
}
fn atomic_file(directory: &Path, name: &str, data: &[u8]) -> Result<(), String> {
    // create_new refuses symlinks and never overwrites a pre-existing temporary.
    let temp = temporary_path(directory, name)?;
    let write = || -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(data)?;
        // IDBFS fsync flushes the entire mount, including temporary files.
        // Web commits once after BOTH final renames, in browser::persist().
        #[cfg(not(target_os = "emscripten"))]
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, directory.join(name))?;
        #[cfg(not(target_os = "emscripten"))]
        File::open(directory)?.sync_all()?;
        Ok(())
    };
    let result=write().map_err(|e|format!("Native save I/O failed ({name}): {e}. Stop and reload the profile before any further action."));
    // On failure leave the temp for diagnosis; never remove a pre-existing file.
    if result.is_ok() {
        debug_assert!(!temp.exists());
    }
    result
}
/// Preserve the already-fsynced previous inode instead of rewriting and
/// fsyncing its complete bytes on every turn. Both names are in one directory.
/// Sync the backup rename before replacing state.json, retaining the existing
/// crash-recovery ordering. No published in-memory state changes on failure.
#[cfg(not(target_os = "emscripten"))]
fn backup_file(directory: &Path) -> Result<(), String> {
    let temp = temporary_path(directory, "state.bak")?;
    let backup = || -> std::io::Result<()> {
        fs::hard_link(directory.join("state.json"), &temp)?;
        fs::rename(&temp, directory.join("state.bak"))?;
        File::open(directory)?.sync_all()
    };
    backup().map_err(|e|format!("Native save I/O failed (state.bak): {e}. Stop and reload the profile before any further action."))
}
pub struct Profile {
    directory: PathBuf,
    _lock: File,
    pub state: State,
    /// A compatible backup was validated and restored; the UI must tell users.
    pub recovered_from: Option<String>,
    saved: Vec<u8>,
}
impl Profile {
    pub fn open(
        directory: &Path,
        scene: &Scene,
        seed: &str,
        test_only: bool,
    ) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let lock_path = directory.join("profile.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| !m.file_type().is_file()) {
            return Err("Profile lock must be a regular file".into());
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "emscripten"))]
        lock.try_lock()
            .map_err(|e| format!("Native profile is already in use (or lock unavailable): {e}"))?;
        let mut existing = read_regular(&directory.join("state.json"))?;
        let current = existing.as_ref().map(|bytes| decode(bytes, scene, seed));
        let missing = existing.is_none();
        let damaged = current.as_ref().is_some_and(Result::is_err)
            && existing.as_ref().is_some_and(|bytes| damaged_write(bytes));
        let mut recovered_from = None;
        if missing || damaged {
            let mut backups_exist = false;
            for name in ["state.bak", "state.checkpoint.bak"] {
                let Some(bytes) = read_regular(&directory.join(name))? else {
                    continue;
                };
                backups_exist = true;
                if !decode(&bytes, scene, seed).is_ok_and(|(state, _)| state.test_only == test_only)
                {
                    continue;
                }
                if let Some(original) = &existing {
                    // Preserve the exact broken bytes for manual repair/export.
                    let quarantine = temporary_path(&directory, "state.damaged")?;
                    let name = quarantine.file_name().unwrap().to_string_lossy();
                    atomic_file(&directory, &name, original)?;
                }
                atomic_file(&directory, "state.json", &bytes)?;
                crate::browser::persist()?;
                eprintln!("Recovered save from {name}; the last uncommitted actions may be lost");
                crate::browser::save_recovered();
                existing = Some(bytes);
                recovered_from = Some(name.to_string());
                break;
            }
            if recovered_from.is_none() && (damaged || backups_exist) {
                return Err("Save is damaged or missing and no valid compatible backup was found; original files were preserved. Import an exported save to recover; refusing to start a new game.".into());
            }
        }
        let (mut state, version_migration) = if recovered_from.is_some() {
            decode(existing.as_ref().unwrap(), scene, seed)?
        } else {
            match current {
                Some(result) => result?,
                None => (State::seed(scene, seed.into())?, false),
            }
        };
        if existing.is_none() {
            state.test_only = test_only;
        }
        if state.test_only != test_only {
            return Err("Diagnostic and ordinary profiles must be separate".into());
        }
        // Keep an additional validated session-start checkpoint. It is not
        // overwritten on every click, nor during recovery from the main backup.
        if recovered_from.is_none()
            && let Some(bytes) = &existing
        {
            atomic_file(&directory, "state.checkpoint.bak", bytes)?;
            crate::browser::persist()?;
        }
        let migrated = state.initialize_systems(scene)?;
        let achievement_migration = state.initialize_achievement_counters(scene)?;
        let changed_seed = state.seed_sha256 != seed;
        if changed_seed {
            state.seed_sha256 = seed.into();
        }
        let mut profile = Self {
            directory,
            _lock: lock,
            state: state.clone(),
            recovered_from,
            saved: existing.unwrap_or_default(),
        };
        if profile.saved.is_empty()
            || migrated
            || achievement_migration
            || changed_seed
            || version_migration
        {
            profile.commit(scene, state)?;
        }
        Ok(profile)
    }
    pub fn commit(&mut self, scene: &Scene, candidate: State) -> Result<(), String> {
        candidate.validate(scene)?;
        if candidate.seed_sha256 != self.state.seed_sha256 {
            return Err("Seed identity cannot change within a profile".into());
        }
        if candidate.test_only != self.state.test_only {
            return Err("Diagnostic mode cannot change within a profile".into());
        }
        let bytes = encode(&candidate)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("Native save is full; action was not committed".into());
        }
        let disk = read_regular(&self.directory.join("state.json"))?.unwrap_or_default();
        if disk != self.saved {
            return Err("Native save changed externally; reload before writing".into());
        }
        if !self.saved.is_empty() {
            #[cfg(not(target_os = "emscripten"))]
            backup_file(&self.directory)?;
            #[cfg(target_os = "emscripten")]
            atomic_file(&self.directory, "state.bak", &self.saved)?;
        }
        // An I/O error after rename is deliberately fatal to the session. Disk
        // may already contain the COMPLETE transaction; retrying in memory is unsafe.
        atomic_file(&self.directory, "state.json", &bytes)?;
        crate::browser::persist()?;
        self.saved = bytes;
        self.state = candidate;
        Ok(())
    }
    pub fn act(
        &mut self,
        scene: &Scene,
        command: &Command,
        wall: i64,
        entropy: impl FnMut() -> Result<u32, String>,
    ) -> Result<Result<String, String>, String> {
        match self.state.apply(scene, command, wall, entropy) {
            Err(rule) => Ok(Err(rule)),
            Ok((next, message)) => {
                self.commit(scene, next)?;
                Ok(Ok(message))
            }
        }
    }
}
#[cfg(target_os = "emscripten")]
pub fn entropy() -> Result<u32, String> {
    Ok(crate::browser::entropy())
}
#[cfg(not(target_os = "emscripten"))]
pub fn entropy() -> Result<u32, String> {
    let mut bytes = [0; 4];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(u32::from_ne_bytes(bytes))
}
