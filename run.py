#!/usr/bin/env python3
"""Prepare the user's IPA, build release Rust and launch an isolated save."""

import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "tools"))
from build import ROOT, build

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--profile",
    default="fresh-review101" if (ROOT / "saves/fresh-review101/state.json").is_file() else "main",
    help="Save name, or absolute directory; use a new name for a new game",
)
parser.add_argument("--no-build", action="store_true")
args, extra = parser.parse_known_args()
binary = ROOT / "target/release/warzone-app-client" if args.no_build else build()
profile = Path(args.profile)
if not profile.is_absolute():
    if profile.name != args.profile or args.profile in (".", ".."):
        parser.error("Use a simple save name or absolute directory")
    profile = ROOT / "saves" / profile
pack = ROOT / "assets/pack"
if profile == ROOT / "saves/main" and (ROOT / "assets/legacy-main/scene.json").is_file():
    pack = ROOT / "assets/legacy-main"
os.environ["SDL_VIDEO_X11_WMCLASS"] = "warzone-client"
os.environ["SDL_VIDEO_WAYLAND_WMCLASS"] = "warzone-client"
os.execv(binary, [str(binary), "--pack", str(pack), "--profile", str(profile), *extra])
