"""Regression checks for the original ambient export semantics."""

import unittest

from animations import compile_symbol


class AmbientExportTests(unittest.TestCase):
    def test_preserves_all_ordered_frames_and_source_rate(self):
        # Sparse Flash frameIndex values are metadata; the native loader creates
        # one AnimationFrame per entry, including empty entries.
        source = {
            "instId": "root",
            "frameRate": 15,
            "frames": [{"frameIndex": i * 4 + 1, "children": []} for i in range(50)],
        }
        clip = compile_symbol(source, {})
        self.assertEqual(clip["fps"], 15)
        self.assertEqual(len(clip["nodes"][0]["frames"]), 50)

    def test_change_keeps_its_position_and_references_original_instance(self):
        source = {
            "instId": "root",
            "frameRate": 8,
            "pos": {"x": 999},
            "frames": [
                {
                    "children": [
                        {
                            "instId": "sprite",
                            "pos": {"x": 3},
                            "pivot": {"x": -2},
                            "imageFileName": "0.png",
                        }
                    ]
                },
                {"children": [{"instId": "sprite", "pos": {"x": 9}}]},
                {"children": [{"instId": "sprite", "pos": {"x": 13}}]},
            ],
        }
        regions = {"0": {"x": 0, "y": 0, "width": 10, "height": 20, "frameX": -1, "frameY": -2}}
        clip = compile_symbol(source, regions, offset=35)
        self.assertEqual(clip["nodes"][0]["pos"], [0, 0])
        self.assertEqual(clip["offset"], [0, 35])
        self.assertEqual(clip["nodes"][1]["sprite"]["rect"], [-3, 2, 10, 20])
        self.assertEqual(clip["nodes"][2], {"kind": "change", "pos": [18, 0], "target": 1})
        self.assertEqual(clip["nodes"][3]["target"], 1)


if __name__ == "__main__":
    unittest.main()
