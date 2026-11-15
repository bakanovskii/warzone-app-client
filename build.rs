// Some Linux installations ship SDL's runtime SONAME without the development
// symlink. Add a private linker alias; never modify /usr or rebuild HyperHLE.
fn main() {
    // Cargo's default watches the entire package, including NEXT.txt/evidence.
    // Only this linker-alias script is an input; Rust sources are tracked by
    // Cargo separately. Documentation/checkpoint edits must not rebuild the game.
    println!("cargo:rerun-if-changed=build.rs");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let assets = root.join("assets/ipa");
    if !assets.join("CANDARA.TTF").is_file() {
        panic!(
            "Place warzone.ipa next to Cargo.toml and run python3 -B tools/prepare.py before compiling"
        )
    }
    println!("cargo:rustc-env=WARZONE_IPA_ASSETS={}", assets.display());
    for file in [
        "CANDARA.TTF",
        "CANDARAB.TTF",
        "CANDARAI.TTF",
        "CANDARAZ.TTF",
        "a_DomIno BoldItalic.ttf",
    ] {
        println!("cargo:rerun-if-changed={}", assets.join(file).display());
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let triplet = match arch.as_str() {
        "x86_64" => "x86_64-linux-gnu",
        "aarch64" => "aarch64-linux-gnu",
        _ => return,
    };
    for base in ["/lib", "/usr/lib"] {
        let source = std::path::PathBuf::from(base)
            .join(triplet)
            .join("libSDL2-2.0.so.0");
        if source.is_file() {
            let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
            let link = out.join("libSDL2.so");
            if !link.exists() {
                std::os::unix::fs::symlink(&source, &link).unwrap();
            }
            println!("cargo:rustc-link-search=native={}", out.display());
            break;
        }
    }
}
