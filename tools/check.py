#!/usr/bin/env python3
"""Run portable native checks sequentially, with one Cargo job."""

import argparse
import json
import os
import shutil
import subprocess
import time

from prepare import ROOT, prepare

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--release", action="store_true", help="Also build the native game and save-balance utility"
)
parser.add_argument(
    "--scripts-only", action="store_true", help="Check Python/web sources without IPA or Cargo"
)
args = parser.parse_args()
if not shutil.which("ruff") or not (ROOT / "node_modules/.bin/biome").is_file():
    parser.error("Install development tools: pip install -r requirements-dev.txt; npm ci")
logs = ROOT / ".local/review"
logs.mkdir(parents=True, exist_ok=True)
env = {**os.environ, "CARGO_BUILD_JOBS": "1"}
checks = [
    ("ruff-lint", ["ruff", "check", "."]),
    ("ruff-format", ["ruff", "format", "--check", "."]),
    ("web-lint-format", ["npm", "run", "check"]),
]
if not args.scripts_only:
    prepare(ROOT / "warzone.ipa", ROOT / "assets/pack")
    checks += [
        ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
        ("clippy", ["cargo", "clippy", "--all-targets", "--message-format=json"]),
        ("tests", ["cargo", "test", "--lib", "--tests"]),
    ]
if args.release and not args.scripts_only:
    checks.append(
        (
            "release",
            [
                "cargo",
                "build",
                "--release",
                "--locked",
                "--bin",
                "warzone-app-client",
                "--example",
                "profile_banknotes",
            ],
        )
    )
results = []
for label, command in checks:
    start = time.monotonic()
    path = logs / ("final-" + label + ".log")
    with path.open("w") as stream:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
    results.append(
        {
            "check": label,
            "exit_code": result.returncode,
            "seconds": round(time.monotonic() - start, 3),
        }
    )
    (logs / "check-results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(label + (": passed" if result.returncode == 0 else ": FAILED; " + str(path)), flush=True)
    if result.returncode:
        print(path.read_text()[-3000:])
        raise SystemExit(result.returncode)
