#!/usr/bin/env python3
"""Build the native client or the Emscripten browser distribution."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path

from prepare import ROOT, prepare


def run(*args, env=None):
    subprocess.run(list(map(str, args)), cwd=ROOT, env=env, check=True)


def loading_layout(source, dist):
    """Keep loader rectangles/type in the IPA coordinate system, including pivots."""
    symbols = json.loads((source / "data/ui/progress/progressscreen.json").read_text())["symbols"]
    selectors = {
        "progressscreen_progress_group_child0.png": ".loading-track",
        "progressscreen_child3.png": ".loading-studio",
        "logo_ru": ".loading-logo",
        "bar": "#progress",
        "percents_txt": "#percent",
        "status_txt": "#loading-status",
    }
    css = ["/* Generated from the original IPA progressscreen.json. */"]
    for layout, key, background in [
        ("long", "progressScreen_long", "loading-background.png"),
        ("phone", "progressScreen", "loading-phone.png"),
        ("ipad", "progressScreen_iPad", "loading-ipad.png"),
    ]:
        root = symbols[key]
        prefix = f'#loading[data-layout="{layout}"]'
        css.append(f'{prefix} {{ background-image: url("{background}?v=__WEB_BUILD__"); }}')
        css.append(
            f"{prefix} .loading-stage {{ width: {root['size']['width']}px; "
            f"height: {root['size']['height']}px; }}"
        )

        def visit(node, x=0, y=0, prefix=prefix):
            pos, pivot = node.get("pos", {}), node.get("pivot", {})
            x, y = x + pos.get("x", 0), y + pos.get("y", 0)
            selector = selectors.get(node.get("id")) or selectors.get(node.get("imageFileName"))
            if selector:
                size = node["size"]
                rules = [
                    f"left: {x + pivot.get('x', 0):g}px",
                    f"top: {y + pivot.get('y', 0):g}px",
                    f"width: {size['width']:g}px",
                    f"height: {size['height']:g}px",
                ]
                if "fontSize" in node:
                    rules += [
                        f"font-size: {node['fontSize']}px",
                        f"line-height: {node['lineHeight']}px",
                        f"text-align: {node['align']}",
                    ]
                css.append(f"{prefix} {selector} {{ {'; '.join(rules)}; }}")
            for child in node.get("children", []):
                visit(child, x, y)

        visit(root)
    (dist / "loading-layout.css").write_text("\n".join(css) + "\n")


def web_shell(dist):
    for name in ("index.html", "style.css", "app.js", "audio.js", "manifest.webmanifest"):
        shutil.copyfile(ROOT / "web" / name, dist / name)
    source = ROOT / "assets/ipa"
    for name, relative in {
        "loading-background.png": "data/ui/progress/progressscreen_long_child0.png",
        "loading-phone.png": "data/ui/progress/progressscreen_child0.png",
        "loading-ipad.png": "data/ui/progress/progressscreen_ipad_child0.png",
        "loading-logo.png": "data/ui/progress/progressscreen_logo_ru.png",
        "loading-track.png": "data/ui/progress/progressscreen_progress_group_child0.png",
        "loading-studio.png": "data/ui/progress/progressscreen_child3.png",
        "loading-bar.png": "data/ui/progress/progressscreen_progress_group_bar.png",
        "button-green.png": "data/ui/messagebox/bcm_gui_messagebox_ok_btn.png",
        "candara.ttf": "CANDARA.TTF",
        "candara-bold.ttf": "CANDARAB.TTF",
        "franklin.ttf": "FRANKLIN GOTHIC DEMI COND REGULAR.TTF",
        "domino.ttf": "a_DomIno BoldItalic.ttf",
        "cafeta.ttf": "CAFETA_.TTF",
    }.items():
        shutil.copyfile(source / relative, dist / name)
    loading_layout(source, dist)
    poster = ROOT / "web/branding/base-poster.jpg"
    if poster.is_file():
        shutil.copyfile(poster, dist / "base-poster.jpg")
    if not (dist / "base-poster.jpg").is_file():
        run(
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-i",
            dist / "loading-background.png",
            "-frames:v",
            "1",
            "-threads",
            "1",
            dist / "base-poster.jpg",
        )
    (dist / "base.mp4").unlink(missing_ok=True)
    (dist / "welcome.jpg").unlink(missing_ok=True)
    favicon = ROOT / "web/branding/favicon.ico"
    if favicon.is_file():
        shutil.copyfile(favicon, dist / "favicon.ico")
    else:
        run(
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-i",
            ROOT / "assets/pack/app-icon.png",
            "-vf",
            "scale=32:32",
            "-frames:v",
            "1",
            "-threads",
            "1",
            dist / "favicon.ico",
        )
    for size in (192, 512):
        run(
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-i",
            ROOT / "assets/pack/app-icon.png",
            "-vf",
            f"scale={size}:{size}",
            "-frames:v",
            "1",
            "-threads",
            "1",
            dist / f"icon-{size}.png",
        )
    sha = hashlib.sha256()
    sha.update((ROOT / "web/sw.js").read_bytes())
    files = sorted(path for path in dist.iterdir() if path.is_file() and path.name != "sw.js")
    for path in files:
        sha.update(path.name.encode())
        with path.open("rb") as stream:
            while data := stream.read(1024 * 1024):
                sha.update(data)
    tag = sha.hexdigest()[:20]
    for name in ("index.html", "style.css", "loading-layout.css", "app.js", "manifest.webmanifest"):
        path = dist / name
        path.write_text(path.read_text().replace("__WEB_BUILD__", tag))
    script = (ROOT / "web/sw.js").read_text().replace("__BUILD_HASH__", tag)
    script = script.replace(
        "__PRECACHE_FILES__", json.dumps(["./", *[p.name + "?v=" + tag for p in files]])
    )
    (dist / "sw.js").write_text(script)


def build(web=False):
    prepare(ROOT / "warzone.ipa", ROOT / "assets/pack")
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "1"
    if not web:
        run("cargo", "build", "--release", "--locked", env=env)
        return ROOT / "target/release/warzone-app-client"
    sdk = Path(env.get("EMSDK", ROOT / ".tools/emsdk"))
    emdir = sdk / "upstream/emscripten"
    if not (emdir / "emcc").is_file():
        raise SystemExit(
            "Install/activate emsdk, source emsdk_env.sh and rustup target add wasm32-unknown-emscripten"
        )
    env.update(EMSDK=str(sdk), EMCC_CORES="1", BINARYEN_CORES="1")
    env["PATH"] = str(emdir) + os.pathsep + env["PATH"]
    node = sorted((sdk / "node").glob("*/bin/node"))
    if node:
        env["EMSDK_NODE"] = str(node[-1])
    obj = ROOT / ".local/bridge.o"
    obj.parent.mkdir(exist_ok=True)
    run(emdir / "emcc", "-O2", "-c", ROOT / "web/bridge.c", "-o", obj, env=env)
    flags = [
        "-Cpanic=abort",
        "-Ctarget-feature=-exception-handling",
        "-Clink-arg=" + str(obj),
        "-Clink-arg=-O1",
        "-Clink-arg=-fno-exceptions",
    ]
    # Only the direct save and frame bridge calls suspend. Rust's indirect
    # entry/backtrace wrappers must still be explicitly instrumented below.
    # SDL's internal
    # swap sleep is disabled and audio is Web Audio, not an indirect callback.
    # Avoid instrumenting every font/render/serde vtable as potentially async.
    settings = [
        "USE_SDL=2",
        "ASYNCIFY=1",
        "ASYNCIFY_IGNORE_INDIRECT=1",
        "ASYNCIFY_ADD=['*lang_start*','*__rust_begin_short_backtrace*','*call_once*','*do_call*','*main*']",
        "ALLOW_MEMORY_GROWTH=1",
        "INITIAL_MEMORY=134217728",
        "MAXIMUM_MEMORY=1073741824",
        "STACK_SIZE=8388608",
        "ASYNCIFY_STACK_SIZE=131072",
        "MODULARIZE=1",
        "EXPORT_NAME=createWarzone",
        "ENVIRONMENT=web",
        "FORCE_FILESYSTEM=1",
        "LZ4=1",
        "EXPORTED_FUNCTIONS=['_main','_wz_cancel_pointer','_wz_validate_import']",
        "EXPORTED_RUNTIME_METHODS=['FS','IDBFS','addRunDependency','removeRunDependency','callMain','setCanvasSize']",
    ]
    flags += ["-Clink-arg=-s" + s for s in settings]
    flags += [
        "-Clink-arg=-lidbfs.js",
        "-Clink-arg=--preload-file",
        "-Clink-arg=" + str(ROOT / "assets/pack") + "@/assets",
    ]
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    # Rebuild std with this SDK's ABI and panic=abort; prebuilt std can pull in
    # legacy WASM exceptions, which Asyncify explicitly cannot instrument.
    run(
        "cargo",
        "+" + env.get("WARZONE_WEB_TOOLCHAIN", "nightly"),
        "build",
        "-Zbuild-std=std,panic_abort",
        "--profile",
        "web",
        "--locked",
        "--target",
        "wasm32-unknown-emscripten",
        "--bin",
        "warzone-app-client",
        env=env,
    )
    dist = ROOT / "web/dist"
    dist.mkdir(parents=True, exist_ok=True)
    source = ROOT / "target/wasm32-unknown-emscripten/web"
    for suffix in ("js", "wasm", "data"):
        # Cargo renames the JS entry point; emcc sidecars keep the Rust stem
        # and its .data package remains in deps/ on this target.
        candidates = [
            directory / (stem + "." + suffix)
            for directory in (source, source / "deps")
            for stem in ("warzone-app-client", "warzone_app_client")
        ]
        src = next((p for p in candidates if p.is_file()), None)
        if src is None:
            raise FileNotFoundError("Missing Emscripten output: " + suffix)
        if suffix == "js":
            script = src.read_text()
            for stem in ("warzone-app-client", "warzone_app_client"):
                script = script.replace(stem + ".wasm", "game.wasm").replace(
                    stem + ".data", "game.data"
                )
            (dist / "game.js").write_text(script)
        else:
            shutil.copyfile(src, dist / ("game." + suffix))
    web_shell(dist)
    return dist


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--web", action="store_true")
    parser.add_argument(
        "--web-shell-only",
        action="store_true",
        help="Repackage web HTML/art without rebuilding WASM",
    )
    args = parser.parse_args()
    if args.web_shell_only:
        dist = ROOT / "web/dist"
        if not (dist / "game.wasm").is_file():
            parser.error("Build --web first")
        web_shell(dist)
        print(dist)
    else:
        print(build(args.web))
