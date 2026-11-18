# Changelog

## 0.2.2 (19 Nov 2026)

### Bugfixes

- Fill the academy progress bar to the end at 100%.
- Center milestone upgrade cards and align dashed dependency arrows and round turn markers.
- Keep academy branches aligned across columns and show cost reductions before training time changes.
- Keep long training effects within their cards and correct the branch order for scouts, Hyena and ST-105.
- Align single-line material names on laboratory cards.
- Keep text crisp when scrollable panels switch between stationary and moving.
- Start the web client on browsers that disallow synchronous WebAssembly instantiation.

## 0.2.1 (18 Nov 2026)

### Bugfixes

- Keep the oil counter in sync with inventory after leaving a campaign mission.
- Keep the academy tree and its scroll position visible behind upgrade dialogs.

## 0.2.0 (17 Nov 2026)

### Improvements

- Check a small release-hash manifest at startup and reuse the installed page when unchanged or the server is unreachable.

### Bugfixes

- Trigger mission checkpoints along the entire movement route, including crossing a zone without stopping; widen one-cell story checkpoints into forgiving strips.
- Align battle object hitboxes with their cells, including lean render views.
- Reuse unchanged web resources across releases, avoid duplicate WASM and asset-package downloads, and retain completed files after interrupted caching.
- Repair incomplete offline caches and distinguish local resource reads from network downloads in the loader.
- Request persistent browser storage on game launch and use strict IndexedDB save durability where supported.
- Load browser saves after filesystem initialization and tolerate damaged file metadata without silently creating a new profile.
- Recover damaged or missing saves from validated backups, preserve broken files, and retain an additional session checkpoint.
- Commit browser saves and backups together, without intermediate whole-filesystem flushes.

## 0.1.2 (16 Nov 2026)

### Bugfixes

- Restore complete ambient animations and original frame rates for all unit models and buildings, including independent nested loops.
- Give map objects independent idle phases so identical squads animate asynchronously without restarting on selection or scene updates.
- Preserve complete ambient timelines when applying or changing the interface language.
- Translate the mission replay button into English and Chinese.

## 0.1.1 (15 Nov 2026)

### Improvements

- Improve league rules scrolling with smooth page settling, preloaded page artwork, and reduced rendering overhead.

### Bugfixes

- Extend the league rules panel across the full viewport width.
- Keep the campaign map and its position behind the mission briefing.
- Show the original warning dialog when starting a mission without an army.

## 0.1.0 (15 Nov 2026)

### Features

- Initial version

<!-- ## 0.1.0 (01 Jun 1970)

### Features

### Improvements

### Bugfixes

### CI/CD

-->
