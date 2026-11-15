#!/usr/bin/env python3
"""Build local assets from the user's unchanged Warzone 1.11.2 IPA."""

import argparse
import hashlib
import json
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def prepare(ipa, output):
    manifest = json.loads((ROOT / "tools/assets.json").read_text())
    if digest(ipa) != manifest["ipa_sha256"]:
        raise ValueError("Expected the supported, unchanged Warzone 1.11.2 IPA")
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    # Reusing an already verified complete pack never touches profiles.
    marker = output / "build.json"
    inputs = [ROOT / "tools/layout.json", ROOT / "tools/assets.json", Path(__file__)]
    identity = {p.name: digest(p) for p in inputs}
    if marker.is_file() and json.loads(marker.read_text()).get("inputs") == identity:
        verify(output)
        return output
    if output.exists():
        raise ValueError("Pack is stale/incomplete; choose a NEW --output; never overwrite a save")
    with tempfile.TemporaryDirectory(prefix="warzone-pack-", dir=output.parent) as tmp:
        staging = Path(tmp) / "pack"
        staging.mkdir()
        (staging / "assets").mkdir()
        source = ROOT / "assets/ipa"
        source.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(ipa) as bundle:
            # No executable, account data, signing files or arbitrary ZIP paths.
            for name in bundle.namelist():
                prefix = "Payload/warzone.app/"
                if not name.startswith(prefix) or name.endswith("/"):
                    continue
                relative = Path(name[len(prefix) :])
                if any(p in ("..", ".") for p in relative.parts) or relative.is_absolute():
                    raise ValueError("Unsafe IPA path")
                if relative.parts[0] == "data" or (
                    len(relative.parts) == 1 and relative.suffix.lower() == ".ttf"
                ):
                    target = source / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(bundle.read(name))
            namespace = "{urn:schemas-microsoft-com:office:spreadsheet}"
            rows = list(
                ET.fromstring((source / "data/configs/lang.xml").read_bytes()).iter(
                    namespace + "Row"
                )
            )
            cells = {}
            for row_id, row in enumerate(rows):
                col = 1
                for cell in row.findall(namespace + "Cell"):
                    col = int(cell.get(namespace + "Index", col))
                    cells[row_id, col] = "".join(cell.itertext()).strip().replace("\\n", "\n")
                    col += 1

            layout = json.loads((ROOT / "tools/layout.json").read_text())
            decoded = {}

            def decode(value):
                if isinstance(value, dict):
                    if list(value) == ["$shared"]:
                        index = value["$shared"]
                        if index not in decoded:
                            decoded[index] = decode(layout["shared"][index])
                        return decoded[index]
                    if list(value) == ["$ipa_text"]:
                        return cells[tuple(value["$ipa_text"])]
                    return {k: decode(v) for k, v in value.items()}
                if isinstance(value, list):
                    return [decode(v) for v in value]
                return value

            scene = decode(layout["root"])
            for name, entry in manifest["files"].items():
                raw = (
                    bundle.read(entry["ipa"])
                    if "ipa" in entry
                    else (ROOT / entry["local"]).read_bytes()
                )
                if hashlib.sha256(raw).hexdigest() != entry["sha256"]:
                    raise ValueError("Asset checksum mismatch: " + name)
                (staging / name).write_bytes(raw)
            # Original icon also supplies desktop/PWA icons; no synthetic artwork.
            (staging / "app-icon.png").write_bytes(bundle.read(manifest["icon"]))
            for name, clip in scene["audio"]["clips"].items():
                original = source / "data" / clip["source"]
                if digest(original) != clip["source_sha256"]:
                    raise ValueError("Audio source checksum mismatch")
                subprocess.run(
                    [
                        "ffmpeg",
                        "-nostdin",
                        "-v",
                        "error",
                        "-threads",
                        "1",
                        "-i",
                        str(original),
                        "-vn",
                        "-threads",
                        "1",
                        "-ac",
                        "2",
                        "-ar",
                        "44100",
                        "-f",
                        "s16le",
                        str(staging / name),
                    ],
                    check=True,
                    timeout=120,
                )
                clip["sha256"] = digest(staging / name)
                clip["frames"] = (staging / name).stat().st_size // 4
            scene["provenance"]["byoa"]["ipa_sha256"] = manifest["ipa_sha256"]
            (staging / "scene.json").write_text(
                json.dumps(scene, ensure_ascii=False, separators=(",", ":")) + "\n"
            )
            (staging / "build.json").write_text(
                json.dumps({"inputs": identity, "ipa_sha256": manifest["ipa_sha256"]}, indent=2)
                + "\n"
            )
            verify(staging)
            staging.rename(output)
    return output


def verify(pack):
    scene = json.loads((pack / "scene.json").read_text())
    expected = {k: v["sha256"] for k, v in scene["textures"].items()}
    expected[scene["font"]] = scene["provenance"]["font_sha256"]
    expected[scene["hud"]["bold_font"]] = scene["hud"]["bold_font_sha256"]
    expected.update({v["path"]: v["sha256"] for v in scene["presentation"]["fonts"].values()})
    expected.update({k: v["sha256"] for k, v in scene["audio"]["clips"].items()})
    for name, sha in expected.items():
        path = (pack / name).resolve()
        if not path.is_relative_to(pack.resolve()) or digest(path) != sha:
            raise ValueError("Invalid prepared asset: " + name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ipa", type=Path, default=ROOT / "warzone.ipa")
    parser.add_argument("--output", type=Path, default=ROOT / "assets/pack")
    args = parser.parse_args()
    print(prepare(args.ipa, args.output))
