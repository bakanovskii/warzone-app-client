"""A shell release must not invalidate the unchanged game download."""

import json
import tempfile
import unittest
from pathlib import Path

from web_cache import version_distribution


class WebCacheTests(unittest.TestCase):
    def distribution(self, root, *, title="Game", wasm=b"wasm-v1"):
        root.mkdir()
        for name, data in {
            "game.data": b"unchanged-large-resource-package",
            "game.wasm": wasm,
            "game.js": b"runtime",
            "logo.png": b"logo",
        }.items():
            (root / name).write_bytes(data)
        (root / "app.js").write_text("const assets = __ASSET_VERSIONS__;")
        (root / "style.css").write_text('body { background: url("logo.png?v=__WEB_BUILD__"); }')
        (root / "index.html").write_text(
            f"<title>{title}</title>"
            '<link href="style.css?v=__WEB_BUILD__">'
            '<script src="app.js?v=__WEB_BUILD__"></script>'
        )
        versions = version_distribution(
            root, 'const CACHE="__BUILD_HASH__"; const FILES=__PRECACHE_FILES__;'
        )
        return versions

    def test_independent_resource_versions_and_recursive_references(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = self.distribution(root / "before")
            after = self.distribution(root / "after", title="New release", wasm=b"wasm-v2")
            self.assertEqual(before["game.data"], after["game.data"])
            self.assertEqual(before["style.css"], after["style.css"])
            self.assertNotEqual(before["game.wasm"], after["game.wasm"])
            self.assertNotEqual(before["app.js"], after["app.js"])
            self.assertNotEqual(before["index.html"], after["index.html"])
            self.assertNotEqual(
                (root / "before/version.json").read_bytes(),
                (root / "after/version.json").read_bytes(),
            )
            self.assertIn(after["game.data"], (root / "after/app.js").read_text())
            self.assertIn(after["app.js"], (root / "after/index.html").read_text())
            self.assertIn(after["logo.png"], (root / "after/style.css").read_text())
            self.assertNotIn("__WEB_BUILD__", (root / "after/index.html").read_text())

    def test_same_inputs_produce_identical_worker_and_urls(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertEqual(self.distribution(root / "a"), self.distribution(root / "b"))
            self.assertEqual((root / "a/sw.js").read_bytes(), (root / "b/sw.js").read_bytes())
            manifest = json.loads((root / "a/version.json").read_text())
            self.assertEqual(len(manifest["build"]), 20)
            self.assertIn(manifest["build"], (root / "a/sw.js").read_text())
            self.assertEqual(
                (root / "a/version.json").read_bytes(), (root / "b/version.json").read_bytes()
            )

    def test_previous_generated_manifest_does_not_change_build_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.distribution(root / "a")
            # Shell-only builds can leave a previous distribution's probe here.
            (root / "a/version.json").write_text('{"build":"obsolete"}')
            versions = version_distribution(
                root / "a", 'const CACHE="__BUILD_HASH__"; const FILES=__PRECACHE_FILES__;'
            )
            self.distribution(root / "b")
            self.assertNotIn("version.json", versions)
            self.assertEqual((root / "a/sw.js").read_bytes(), (root / "b/sw.js").read_bytes())
            self.assertEqual(
                (root / "a/version.json").read_bytes(), (root / "b/version.json").read_bytes()
            )
