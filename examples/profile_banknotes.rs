//! Explicit local testing balance edit through the same locked/checksummed save API.
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use warzone_app_client::{
    Scene,
    save::{Profile, digest},
};
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut pack, mut directory, mut change) = (None, None, None);
    let mut dry = false;
    while let Some(key) = args.next() {
        match key.as_str() {
            "--pack" => pack = Some(PathBuf::from(args.next().ok_or("Missing --pack")?)),
            "--profile" => directory = Some(PathBuf::from(args.next().ok_or("Missing --profile")?)),
            "--add" | "--set" => {
                if change.is_some() {
                    return Err("Choose exactly one of --add/--set".into());
                }
                let n = args
                    .next()
                    .ok_or("Missing amount")?
                    .parse::<i64>()
                    .map_err(|_| "Invalid amount")?;
                if !(0..i32::MAX as i64).contains(&n) {
                    return Err("Amount outside supported range".into());
                }
                change = Some((key == "--add", n));
            }
            "--dry-run" => dry = true,
            _ => return Err(format!("Unknown argument: {key}")),
        }
    }
    let pack = pack.ok_or("--pack required")?;
    let directory = directory.ok_or("--profile required")?;
    if !directory.join("state.json").is_file() {
        return Err("Existing native profile required; refusing to create or reset it".into());
    }
    let (add, amount) = change.ok_or("--add AMOUNT or --set AMOUNT required")?;
    let scene = Scene::load(&pack)?;
    let seed = digest(&fs::read(pack.join("scene.json")).map_err(|e| e.to_string())?);
    let mut profile = Profile::open(&directory, &scene, &seed, false)?;
    let before = profile.state.resources["realMoney"];
    let after = if add {
        before.checked_add(amount).ok_or("Balance overflow")?
    } else {
        amount
    };
    if !(0..i32::MAX as i64).contains(&after) {
        return Err("Resulting balance outside supported range".into());
    }
    if dry || before == after {
        println!("Banknotes: {before} -> {after}; no balance change");
        return Ok(());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let backup = directory.join(format!("banknotes-before-{stamp}"));
    fs::create_dir(&backup).map_err(|e| e.to_string())?;
    for name in ["state.json", "state.bak"] {
        let from = directory.join(name);
        if from.is_file() {
            let bytes = fs::read(&from).map_err(|e| e.to_string())?;
            fs::write(backup.join(name), bytes).map_err(|e| e.to_string())?;
        }
    }
    let mut next = profile.state.clone();
    next.resources.insert("realMoney".into(), after);
    profile.commit(&scene, next)?;
    let audit = serde_json::json!({"before":before,"after":after,"mode":if add{"add"}else{"set"},"requested_amount":amount,"changed_key":"resources.realMoney","profile":directory,"pack":pack,"backup":backup,"revision":profile.state.revision});
    fs::write(
        backup.join("operation.json"),
        serde_json::to_vec_pretty(&audit).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!("{}", audit);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Banknote edit refused: {e}");
        std::process::exit(1)
    }
}
