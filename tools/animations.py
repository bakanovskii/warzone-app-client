"""Compile complete ambient AnimationManager graphs from the supplied IPA.

Frames are ordered array entries, not Flash frameIndex timestamps. Nested
MovieClips and AnimationChange references use the symbol's clock independently.
Keep graphs rather than baking a truncated (or huge least-common-period) loop.
"""

import hashlib
import json
import math
import xml.etree.ElementTree as ET


def compile_symbol(symbol, regions, offset=0):
    fps = float(symbol["frameRate"])
    if not math.isfinite(fps) or not 0 < fps <= 240:
        raise ValueError("Invalid ambient frame rate")
    nodes, instances = [], {}

    def point(node, key):
        result = [float(node.get(key, {}).get(axis, 0)) * 2 for axis in ("x", "y")]
        if any(not math.isfinite(x) or abs(x) > 100000 for x in result):
            raise ValueError("Invalid ambient geometry")
        return result

    def visit(node, depth=0):
        if depth > 24 or len(nodes) >= 12000:
            raise ValueError("Ambient graph exceeds supported size")
        uid = str(node["instId"])
        index = len(nodes)
        result = {"pos": point(node, "pos")}
        nodes.append(result)
        if "frames" in node:
            instances[uid] = index
            result.update(kind="clip", frames=[])
            for frame in node["frames"]:
                children = frame.get("children", [frame] if "imageFileName" in frame else [])
                result["frames"].append([visit(child, depth + 1) for child in children])
            if not result["frames"]:
                raise ValueError("Empty ambient clip")
        elif "imageFileName" in node:
            instances[uid] = index
            region = regions[node["imageFileName"].rsplit(".", 1)[0]]
            x, y = point(node, "pivot")
            result.update(
                kind="image",
                sprite={
                    "src": [region[k] for k in ("x", "y", "width", "height")],
                    "rect": [
                        x - region["frameX"],
                        y - region["frameY"],
                        region["width"],
                        region["height"],
                    ],
                },
            )
        else:
            result.update(kind="change", target=instances[uid])
        return index

    root = visit(symbol)
    # SPAnimation.render calls the root directly; root pos is not a transform.
    nodes[root]["pos"] = [0, 0]
    return {"fps": fps, "offset": [0, offset], "roots": [root], "nodes": nodes}


def build(bundle, scene, manifest):
    textures = {
        entry["ipa"]: texture
        for texture, entry in manifest["files"].items()
        if "ipa" in entry and texture in scene["textures"]
    }
    # Older prepared atlases were copied via recovery paths. Identity is the
    # original PNG hash, so no recovery directory is needed in a clean checkout.
    hashes = {info["sha256"]: texture for texture, info in scene["textures"].items()}
    names = set(bundle.namelist())
    clips, sources = [], {}
    for path in sorted(names):
        unit = "/data/game/units/" in path
        building = "/data/game/animations/" in path
        if not (unit or building) or not path.endswith(("_idle.json", "_working.json")):
            continue
        png, xml = path[:-5] + ".png", path[:-5] + ".xml"
        if png not in names or xml not in names:
            continue
        texture = textures.get(png)
        if texture is None:
            texture = hashes.get(hashlib.sha256(bundle.read(png)).hexdigest())
        if texture is None:
            continue
        raw, atlas = bundle.read(path), bundle.read(xml)
        sources[path] = hashlib.sha256(raw).hexdigest()
        sources[xml] = hashlib.sha256(atlas).hexdigest()
        regions = {}
        for region in ET.fromstring(atlas):
            if region.get("rotated", "false") != "false":
                raise ValueError("Rotated ambient atlas entry")
            regions[region.get("name")] = {
                k: int(region.get(k, 0)) for k in ("x", "y", "width", "height", "frameX", "frameY")
            }
        symbols = json.loads(raw)["symbols"]
        graphs = {}
        for name, symbol in symbols.items():
            graph = compile_symbol(symbol, regions, 35 if unit else 0)
            graphs[name] = graph
            clips.append({"texture": texture, "symbol": name, **graph})
        if unit:
            for name, body in graphs.items():
                if "_shadow_" in name:
                    continue
                prefix, direction, pose = name.rsplit("_", 2)
                shadow = graphs.get(f"{prefix}_shadow_{direction}_{pose}")
                if shadow is None:
                    continue
                # Battle/upgraded artwork stores shadow and body in one Track;
                # base artwork stores them in separate Tracks. Bind both forms.
                count = len(shadow["nodes"])
                shifted = []
                for node in body["nodes"]:
                    node = dict(node)
                    if node["kind"] == "clip":
                        node["frames"] = [[i + count for i in f] for f in node["frames"]]
                    elif node["kind"] == "change":
                        node["target"] += count
                    shifted.append(node)
                clips.append(
                    {
                        "texture": texture,
                        "symbol": name + "+shadow",
                        "fps": body["fps"],
                        "offset": body["offset"],
                        "roots": shadow["roots"] + [i + count for i in body["roots"]],
                        "nodes": shadow["nodes"] + shifted,
                    }
                )
    if not clips:
        raise ValueError("No original ambient animations found")
    return {"format": "warzone-ambient-v1", "clips": clips, "sources": sources}
