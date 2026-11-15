Optional local files: `base-poster.jpg` and `favicon.ico`. These are excluded
from Git alongside the other game artwork. The working copy uses a clean,
high-quality screenshot of this client's base and the owner's favicon. A clean
BYOA build falls back to the original IPA artwork/icon.

With the web server running, capture your own base from a read-only save import:

```sh
WARZONE_CAPTURE_BACKGROUND=1 WARZONE_WIDE=1 WARZONE_IMPORT_SAVE=saves/<name>/state.json node tools/browser_smoke.mjs
python3 -B tools/build.py --web-shell-only
```

This uses a temporary browser profile, hides interface layers without changing
the source save, and captures a 3840×2160 JPEG directly from the renderer. It
restores the test viewport afterwards. The normal smoke check does not overwrite
artwork. Loading artwork, layout, buttons and fonts always come from the IPA.
