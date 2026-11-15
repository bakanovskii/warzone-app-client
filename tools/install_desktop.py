#!/usr/bin/env python3
"""Install a launcher for this checkout in the current user's application menu."""

import shutil
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
icon = root / "assets/pack/app-icon.png"
if not icon.is_file():
    raise SystemExit("First run tools/prepare.py")
apps = Path.home() / ".local/share/applications"
apps.mkdir(parents=True, exist_ok=True)


def quoted(value):
    # Desktop Entry Exec double quoting, including literal percent field codes.
    return (
        '"'
        + str(value)
        .replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("`", "\\`")
        .replace("$", "\\$")
        .replace("%", "%%")
        + '"'
    )


content = "\n".join(
    [
        "[Desktop Entry]",
        "Type=Application",
        "Name=Warzone — Линия Огня",
        "Comment=Локальная база и кампания",
        f"Exec={quoted(sys.executable)} -B {quoted(root / 'run.py')} --no-build",
        f"Path={root}",
        f"Icon={icon}",
        "Terminal=false",
        "Categories=Game;StrategyGame;",
        "StartupWMClass=warzone-client",
        "StartupNotify=true",
        "",
    ]
)
path = apps / "warzone-client.desktop"
path.write_text(content)
path.chmod(0o755)
if shutil.which("update-desktop-database"):
    subprocess.run(["update-desktop-database", str(apps)], check=False)
print(path)
