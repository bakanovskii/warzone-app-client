// Local runtime check; Node 22+ and Chromium are required. No npm dependencies.
import { spawn } from "node:child_process";
import { appendFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const out = resolve(".local/review/browser");
await mkdir(out, { recursive: true });
if (typeof WebSocket === "undefined")
  throw Error("Browser check requires Node 22+ (the emsdk Node binary also works)");
for (const name of ["report.json", "failure.json", "memory.jsonl", "runtime.jsonl"])
  await rm(join(out, name), { force: true });
const profile = await mkdtemp(join(tmpdir(), "warzone-browser-"));
const browser = spawn(
  "/usr/bin/prlimit",
  [
    "--core=0",
    "--",
    process.env.CHROMIUM || "/usr/lib/chromium/chromium",
    "--headless=new",
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--remote-allow-origins=*",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
    "--autoplay-policy=no-user-gesture-required",
    "--remote-debugging-port=0",
    "--no-first-run",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-breakpad",
    "--disable-crash-reporter",
    ...(process.env.WARZONE_LIFTOFF === "1" ? ["--js-flags=--liftoff-only"] : []),
    ...(process.env.WARZONE_WIDE === "1"
      ? ["--window-size=1920,1200"]
      : ["--window-size=1100,800"]),
    `--user-data-dir=${profile}`,
    "about:blank",
  ],
  {
    // Chromium's shutdown signal handlers must not share the test runner's
    // process group (or that of an enclosing resource guard).
    detached: true,
    stdio: ["ignore", "ignore", "pipe"],
    env: { ...process.env, DBUS_SESSION_BUS_ADDRESS: "unix:path=/dev/null", NO_AT_BRIDGE: "1" },
  },
);
const browserExited = new Promise((resolve) => browser.once("exit", resolve));
let browserLog = "";
browser.stderr.on("data", (d) => {
  browserLog += d;
  writeFile(join(out, "chromium-live.log"), browserLog).catch(() => {});
});
let ws,
  seq = 0;
const pending = new Map(),
  events = [];
const cg = (await readFile("/proc/self/cgroup", "utf8")).trim().split("0::")[1];
const memoryTimer = setInterval(async () => {
  try {
    await appendFile(
      join(out, "memory.jsonl"),
      `${JSON.stringify({
        at: Date.now(),
        bytes: +(await readFile(`/sys/fs/cgroup${cg}/memory.current`, "utf8")),
      })}\n`,
    );
  } catch {}
}, 500);
const rpc = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await rpc("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const until = async (test, seconds = 180) => {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    const value = await test();
    if (value) return value;
    await delay(500);
  }
  throw Error("Timed out waiting for browser runtime");
};
const reload = async () => {
  const origin = await evaluate("performance.timeOrigin");
  await rpc("Page.reload");
  await until(async () => {
    try {
      return await evaluate(
        `performance.timeOrigin!==${origin} && document.readyState==='complete' && typeof launch==='function'`,
      );
    } catch (error) {
      if (/context|Inspected target navigated or closed/i.test(String(error))) return false;
      throw error;
    }
  }, 30);
};
try {
  const port = await until(async () => {
    try {
      return (await readFile(join(profile, "DevToolsActivePort"), "utf8")).split("\n")[0];
    } catch {
      return false;
    }
  }, 20);
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
  await new Promise((r, j) => {
    ws.onopen = r;
    ws.onerror = j;
  });
  ws.onmessage = ({ data }) => {
    const m = JSON.parse(data);
    if (m.id) {
      const p = pending.get(m.id);
      pending.delete(m.id);
      m.error ? p.reject(Error(JSON.stringify(m.error))) : p.resolve(m.result);
    } else {
      events.push(m);
      if (m.method.startsWith("Runtime."))
        appendFile(join(out, "runtime.jsonl"), `${JSON.stringify(m)}\n`).catch(() => {});
    }
  };
  await rpc("Page.enable");
  await rpc("Runtime.enable");
  await rpc("Network.enable");
  const wide = process.env.WARZONE_WIDE === "1";
  const phoneMode = process.env.WARZONE_PHONE === "1";
  if (phoneMode) {
    await rpc("Emulation.setDeviceMetricsOverride", {
      width: 390,
      height: 844,
      screenWidth: 390,
      screenHeight: 844,
      deviceScaleFactor: 3,
      mobile: true,
    });
    await rpc("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 2 });
  }
  if (wide)
    await rpc("Emulation.setDeviceMetricsOverride", {
      width: 1920,
      height: 1080,
      screenWidth: 1920,
      screenHeight: 1200,
      deviceScaleFactor: 1,
      mobile: false,
    });
  await rpc("Page.navigate", { url: process.env.WARZONE_WEB_URL || "http://127.0.0.1:8080/" });
  await until(() => evaluate("document.readyState==='complete' && typeof launch==='function'"), 30);
  const screenshot = async (name) => {
    const image = await rpc("Page.captureScreenshot", { format: "png" });
    await writeFile(join(out, `${name}.png`), Buffer.from(image.data, "base64"));
  };
  await evaluate("document.fonts.ready.then(()=>true)");
  const posterSize = await evaluate(`new Promise((resolve, reject) => {
    const image = new Image(); image.onload=()=>resolve([image.naturalWidth,image.naturalHeight]);
    image.onerror=()=>reject(Error('Background image did not load'));
    image.src=asset('base-poster.jpg');
  })`);
  if (await evaluate("!!document.querySelector('video')"))
    throw Error("Video remains on static welcome page");
  const panelCenter = await evaluate(
    "(()=>{const r=document.querySelector('.welcome-panel').getBoundingClientRect();return r.x+r.width/2})()",
  );
  if (Math.abs(panelCenter - (await evaluate("innerWidth/2"))) > 2)
    throw Error("Welcome controls are not centered");
  if ((await evaluate("document.querySelectorAll('.community a svg').length")) !== 2)
    throw Error("Community icon buttons missing");
  for (const id of phoneMode ? [] : ["start", "restore"]) {
    const point = await evaluate(`(() => {
      const r=document.querySelector('#${id}').getBoundingClientRect();
      return {x:r.x+r.width/2,y:r.y+r.height/2};
    })()`);
    await rpc("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
    await until(
      () =>
        evaluate(`getComputedStyle(document.querySelector('#${id}')).filter==='brightness(1.15)'`),
      5,
    );
  }
  await rpc("Input.dispatchMouseEvent", { type: "mouseMoved", x: 0, y: 0 });
  await screenshot("welcome");
  // Capture the source-authored loader throughout its fill, before starting
  // any game download. This also exposes layering errors at high percentages.
  await evaluate(
    "document.querySelector('#welcome').hidden=true;document.querySelector('#loading').hidden=false;fitLoader();document.fonts.ready.then(()=>true)",
  );
  for (const value of [0, 0.25, 0.5, 0.75, 1]) {
    await evaluate(
      `downloads.wasm=${value};downloads.data=${value};showProgress('Загружаем WASM-клиент…')`,
    );
    await screenshot(`loading-${Math.round(value * 100)}`);
  }
  if (wide) {
    for (const [width, height] of [
      [960, 640],
      [1024, 768],
      [844, 390],
      [390, 844],
    ]) {
      await rpc("Emulation.setDeviceMetricsOverride", {
        width,
        height,
        screenWidth: width,
        screenHeight: height,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await evaluate(
        "fitLoader();downloads.wasm=downloads.data=.5;showProgress('Загружаем WASM-клиент…')",
      );
      if (
        await evaluate(`['.loading-logo','#progress','#percent','#loading-status'].some(selector => {
        const r=document.querySelector(selector).getBoundingClientRect();
        return r.left < -1 || r.top < -1 || r.right > innerWidth+1 || r.bottom > innerHeight+1;
      })`)
      )
        throw Error(`Loader clipped at ${width}x${height}`);
      await screenshot(`loading-${width}x${height}`);
      if (width === 390 || width === 844) {
        await evaluate(
          "document.querySelector('#loading').hidden=true;document.querySelector('#welcome').hidden=false",
        );
        await screenshot(`welcome-${width}x${height}`);
        if (await evaluate("document.documentElement.scrollWidth>innerWidth"))
          throw Error("Welcome page overflows horizontally");
        await evaluate(
          "document.querySelector('#loading').hidden=false;document.querySelector('#welcome').hidden=true",
        );
      }
    }
    await rpc("Emulation.setDeviceMetricsOverride", {
      width: 1920,
      height: 1080,
      screenWidth: 1920,
      screenHeight: 1200,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await evaluate("fitLoader()");
  }
  await evaluate(
    "downloads.wasm=downloads.data=0;document.querySelector('#welcome').hidden=false;document.querySelector('#loading').hidden=true",
  );
  // No WASM/resources are fetched merely by opening the landing page.
  if (
    await evaluate(
      "performance.getEntriesByType('resource').some(r=>r.name.includes('/game.wasm')||r.name.includes('/game.data'))",
    )
  )
    throw Error("Game downloaded before Start");
  if (process.env.WARZONE_IMPORT_SAVE) {
    const encoded = (await readFile(process.env.WARZONE_IMPORT_SAVE)).toString("base64");
    await evaluate(
      `importedSave=Uint8Array.from(atob(${JSON.stringify(encoded)}),c=>c.charCodeAt(0)); undefined`,
    );
  }
  await evaluate("document.querySelector('#start').click()");
  if (phoneMode) {
    if (
      !(await evaluate(
        "stage.classList.contains('rotated') && stage.clientWidth===844 && stage.clientHeight===390 && loader.dataset.layout==='long'",
      ))
    )
      throw Error("Phone Start did not switch to a landscape loader");
  }
  await screenshot("loading");
  await until(() =>
    evaluate(
      "(()=>{const error=document.querySelector('#status').textContent;if(error.startsWith('Не удалось'))throw Error(error);return typeof game!=='undefined' && game?.FS && (()=>{try{return game.FS.readFile('/saves/main/state.json').length>0}catch{return false}})()})()",
    ),
  );
  await delay(3000);
  await until(
    () =>
      evaluate(
        "document.querySelector('#loading').hidden && !document.querySelector('#canvas').hidden",
      ),
    30,
  );
  const state = await evaluate(
    "JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'}))",
  );
  const image = await rpc("Page.captureScreenshot", { format: "png" });
  if (image.data.length < 150000) throw Error("Canvas appears empty");
  await writeFile(join(out, "running.png"), Buffer.from(image.data, "base64"));
  if (process.env.WARZONE_CAPTURE_BACKGROUND === "1") {
    if (!wide) throw Error("Background capture needs WARZONE_WIDE=1");
    try {
      await evaluate(
        "game.warzoneBackgroundCapture=true;document.querySelector('#toolbar').hidden=true",
      );
      await rpc("Emulation.setDeviceMetricsOverride", {
        width: 3840,
        height: 2160,
        screenWidth: 3840,
        screenHeight: 2160,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await until(() => evaluate("game.warzoneViewport?.output[0]===3840"), 15);
      // Match the original framing at twice the output resolution. A five-tick
      // zoom is close to 2x; it affects only this temporary imported profile.
      await evaluate("zoom(-500)");
      await delay(1000);
      const picture = await rpc("Page.captureScreenshot", { format: "jpeg", quality: 96 });
      await writeFile("web/branding/base-poster.jpg", Buffer.from(picture.data, "base64"));
    } finally {
      await evaluate("zoom(500)");
      await rpc("Emulation.setDeviceMetricsOverride", {
        width: 1920,
        height: 1080,
        screenWidth: 1920,
        screenHeight: 1200,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await until(() => evaluate("game.warzoneViewport?.output[0]===1920"), 15);
      await evaluate(
        "game.warzoneBackgroundCapture=false;document.querySelector('#toolbar').hidden=false",
      );
      await delay(500);
    }
  }
  let fullscreenToggle = false,
    widePanels = false;
  let phoneRetina = false,
    movableToolbar = false;
  if (wide) {
    // SDL must follow JS buffer changes even though its own web resize path is
    // disabled. Exercise real size and density changes, not only the DOM flag.
    for (const [width, height, density] of [
      [1420, 800, 1],
      [960, 640, 2],
      [1920, 1080, 1],
    ]) {
      await rpc("Emulation.setDeviceMetricsOverride", {
        width,
        height,
        deviceScaleFactor: density,
        screenWidth: 1920,
        screenHeight: 1200,
        mobile: false,
      });
      await until(
        () =>
          evaluate(
            `game.warzoneViewport?.window[0]===${width * density} && game.warzoneViewport?.window[1]===${height * density} && game.warzoneViewport?.output[0]===${width * density} && game.warzoneViewport?.output[1]===${height * density}`,
          ),
        15,
      );
    }
    for (const expected of [true, false]) {
      await rpc("Runtime.evaluate", {
        expression: "document.querySelector('#fullscreen').click()",
        userGesture: true,
      });
      await until(
        () =>
          evaluate(
            `!!document.fullscreenElement===${expected} && document.querySelector('#fullscreen').getAttribute('aria-pressed')==='${expected}'`,
          ),
        10,
      );
    }
    fullscreenToggle = true;
    // An imported save may restore a selected unit and its contextual toolbar.
    // Return to the base toolbar before testing its menu buttons.
    for (const type of ["keyDown", "keyUp"])
      await rpc("Input.dispatchKeyEvent", {
        type,
        key: "Escape",
        code: "Escape",
        windowsVirtualKeyCode: 27,
      });
    await delay(500);
    const scene = JSON.parse(await readFile("assets/pack/scene.json", "utf8"));
    const clickDuration =
      scene.audio.clips[scene.audio.events["ui/button_click"][0].path].frames / 44100;
    const scale = 2.5,
      w = 1920 / scale,
      h = 1080 / scale,
      extra = w - 568;
    const point = (key, id, origin, right = false) => {
      const symbol = scene.interface.symbols[key],
        r = symbol.buttons[id];
      return [
        (origin[0] + r[0] + r[2] / 2 + (right ? extra : 0)) * scale,
        (origin[1] + r[1] + r[3] / 2) * scale,
      ];
    };
    const click = async (p) => {
      await rpc("Input.dispatchMouseEvent", { type: "mouseMoved", x: p[0], y: p[1] });
      await rpc("Input.dispatchMouseEvent", {
        type: "mousePressed",
        button: "left",
        clickCount: 1,
        x: p[0],
        y: p[1],
      });
      await rpc("Input.dispatchMouseEvent", {
        type: "mouseReleased",
        button: "left",
        clickCount: 1,
        x: p[0],
        y: p[1],
      });
      await delay(1000);
    };
    const sidebar = (key, id, column, settings = false) =>
      point(key, id, [w - 31.5 - column * 32.5, h - (settings ? 126 : 94.5)]);
    const layouts = [
      [
        "multiplayer",
        point("main", "battle_btn", [w - scene.interface.symbols.main.size[0] - 5, h - 45]),
        point(
          "pvp_profile",
          "close_btn",
          [44, h - scene.interface.symbols.pvp_profile.size[1] + 4],
          true,
        ),
      ],
      [
        "inventory",
        point("main", "inventory_btn", [w - scene.interface.symbols.main.size[0] - 5, h - 45]),
        point("inventory", "close_btn", [
          237.5 + w - 480,
          h - scene.interface.symbols.inventory.size[1] + 10.5,
        ]),
      ],
      [
        "settings",
        sidebar("settings_button", "settings", 0, true),
        point(
          "settings",
          "cancel_btn",
          [
            (568 - scene.interface.symbols.settings.size[0]) / 2,
            h - scene.interface.symbols.settings.size[1] + 4,
          ],
          true,
        ),
      ],
      [
        "leagues",
        sidebar("leagues_button", "leaderboard", 2),
        point(
          "leagues",
          "close_btn",
          [
            (568 - scene.interface.symbols.leagues.size[0]) / 2,
            h - scene.interface.symbols.leagues.size[1],
          ],
          true,
        ),
      ],
      [
        "achievements",
        sidebar("achievements_button", "achievements", 1),
        point(
          "achievements",
          "close_btn",
          [
            (568 - scene.interface.symbols.achievements.size[0]) / 2,
            h - scene.interface.symbols.achievements.size[1] + 84,
          ],
          true,
        ),
      ],
    ];
    const clickCounts = [];
    await writeFile(
      join(out, "viewport.json"),
      JSON.stringify(
        await evaluate(
          "({native:game.warzoneViewport,css:[innerWidth,innerHeight],canvas:[canvas.width,canvas.height],rect:canvas.getBoundingClientRect().toJSON(),dpr:devicePixelRatio})",
        ),
        null,
        2,
      ),
    );
    await evaluate(
      "window.clickTrace=[];game.warzoneTrace=[];for(const name of ['mousedown','mouseup'])canvas.addEventListener(name,e=>clickTrace.push({type:name,x:e.clientX,y:e.clientY,buttons:e.buttons}),true)",
    );
    for (const [name, open, close] of layouts) {
      await evaluate("if(game.warzoneFrameTimes)game.warzoneFrameTimes.length=0");
      const before = await evaluate("JSON.parse(JSON.stringify(WarzoneAudio.stats))");
      await click(open);
      await screenshot(`wide-${name}`);
      const after = await evaluate("JSON.parse(JSON.stringify(WarzoneAudio.stats))");
      const newPlays = after.plays.slice(before.plays.length);
      if (name === "leagues" && process.env.WARZONE_REVIEW113 === "1") {
        const league = scene.interface.symbols.leagues;
        await click(
          point("leagues", "help_btn", [(568 - league.size[0]) / 2, h - league.size[1]], true),
        );
        await screenshot("wide-leagues-help");
        const help = scene.interface.symbols.leagues_help,
          frame = help.frames.picture_frame,
          cy = (h - help.size[1] + frame[1] + frame[3] / 2) * scale,
          distance = frame[2] * scale * 0.7;
        const measurements = [];
        if (process.env.WARZONE_CPU_PROFILE === "1") {
          await rpc("Profiler.enable");
          await rpc("Profiler.start");
        }
        for (const direction of [-1, 1, -1, 1]) {
          const sx = 960 - (direction * distance) / 2;
          await evaluate("game.warzoneFrameTimes.length=0");
          await rpc("Input.dispatchMouseEvent", {
            type: "mousePressed",
            button: "left",
            buttons: 1,
            clickCount: 1,
            x: sx,
            y: cy,
          });
          for (let i = 1; i <= 20; i++) {
            await rpc("Input.dispatchMouseEvent", {
              type: "mouseMoved",
              button: "left",
              buttons: 1,
              x: sx + (direction * distance * i) / 20,
              y: cy,
            });
            await delay(16);
          }
          await rpc("Input.dispatchMouseEvent", {
            type: "mouseReleased",
            button: "left",
            buttons: 0,
            clickCount: 1,
            x: sx + direction * distance,
            y: cy,
          });
          await delay(500);
          measurements.push(await evaluate("[...game.warzoneFrameTimes]"));
        }
        await writeFile(join(out, "league-help-scroll.json"), JSON.stringify(measurements));
        if (process.env.WARZONE_CPU_PROFILE === "1") {
          const { profile } = await rpc("Profiler.stop");
          await writeFile(join(out, "league-help.cpuprofile"), JSON.stringify(profile));
          await rpc("Profiler.disable");
        }
        for (const type of ["keyDown", "keyUp"])
          await rpc("Input.dispatchKeyEvent", {
            type,
            key: "Escape",
            code: "Escape",
            windowsVirtualKeyCode: 27,
          });
        await delay(250);
      }
      const buttonClicks = newPlays.filter(
        (p) => p.kind === 0 && Math.abs(p.duration - clickDuration) < 0.00001,
      ).length;
      clickCounts.push({
        name,
        started: after.started - before.started,
        buttonClicks,
        plays: newPlays,
        duplicateStarts: after.duplicateStartsIgnored - before.duplicateStartsIgnored,
        longestUpdateMs: await evaluate("Math.max(0,...game.warzoneFrameTimes||[])"),
      });
      if (
        buttonClicks !== 1 ||
        newPlays.some(
          (p, i) =>
            p.kind === 0 &&
            (p.loop || newPlays.slice(0, i).some((q) => q.kind === 0 && q.id === p.id)),
        ) ||
        after.duplicateStartsIgnored !== before.duplicateStartsIgnored
      ) {
        await writeFile(
          join(out, "click-failure.json"),
          JSON.stringify(
            await evaluate(
              "({trace:clickTrace,events:game.warzoneTrace,native:game.warzoneViewport,viewport:visualViewport.scale,screen:[screen.width,screen.height]})",
            ),
            null,
            2,
          ),
        );
        throw Error(
          "Button sound missing/repeated/looping: " +
            name +
            "; " +
            JSON.stringify({ before, after }),
        );
      }
      await click(close);
    }
    await writeFile(join(out, "wide-clicks.json"), JSON.stringify(clickCounts, null, 2));
    if (process.env.WARZONE_REVIEW113 === "1") {
      const key = async (name, code) => {
        for (const type of ["keyDown", "keyUp"])
          await rpc("Input.dispatchKeyEvent", {
            type,
            key: name,
            code: name,
            windowsVirtualKeyCode: code,
          });
        await delay(750);
      };
      await click(
        point("main", "missions_btn", [w - scene.interface.symbols.main.size[0] - 5, h - 45]),
      );
      await screenshot("campaign-map");
      await key("Enter", 13);
      await screenshot("campaign-briefing");
      if (process.env.WARZONE_NO_ARMY === "1") {
        await click(
          point("campaign_briefing", "button_play", [
            (w - scene.interface.symbols.campaign_briefing.size[0]) / 2,
            (h - scene.interface.symbols.campaign_briefing.size[1]) / 2,
          ]),
        );
        await screenshot("campaign-no-army-warning");
        if (
          await evaluate(
            "JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'})).state.mission != null",
          )
        )
          throw Error("An empty army was admitted into a mission");
        const message = scene.interface.symbols.message,
          ok = message.buttons.ok_btn;
        await click([960, ((h - message.size[1]) / 2 + ok[1] + ok[3] / 2) * scale]);
        await screenshot("campaign-warning-dismissed");
      }
      await key("Escape", 27);
      await screenshot("campaign-map-returned");
      await key("Escape", 27);
    }
    await rpc("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 2 });
    const tap = async (point) => {
      await rpc("Input.dispatchTouchEvent", {
        type: "touchStart",
        touchPoints: [{ id: 1, x: point[0], y: point[1], radiusX: 2, radiusY: 2, force: 1 }],
      });
      await rpc("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await delay(750);
    };
    const inventoryLayout = layouts.find(([name]) => name === "inventory");
    await tap(inventoryLayout[1]);
    if ((await evaluate("game.warzoneTrace.at(-1)?.panel")) !== 6)
      throw Error("A first-finger tap did not open Inventory");
    await screenshot("wide-touch-inventory");
    await tap(inventoryLayout[2]);
    if ((await evaluate("game.warzoneTrace.at(-1)?.panel")) !== -1)
      throw Error("Touch close failed");
    await rpc("Emulation.setTouchEmulationEnabled", { enabled: false });
    widePanels = true;
    if (await evaluate("document.querySelector('#save-status')!==null"))
      throw Error("Save toast remains");
    if (await evaluate("!!game.SDL2?.audio"))
      throw Error("Legacy ScriptProcessor audio still active");
  }
  if (phoneMode) {
    await until(
      () =>
        evaluate(
          "game.warzoneViewport?.output[0]===2532 && game.warzoneViewport?.output[1]===1170",
        ),
      20,
    );
    for (const type of ["keyDown", "keyUp"])
      await rpc("Input.dispatchKeyEvent", {
        type,
        key: "Escape",
        code: "Escape",
        windowsVirtualKeyCode: 27,
      });
    await delay(750);
    if (!(await evaluate("actions.hidden && handle.getAttribute('aria-expanded')==='false'")))
      throw Error("Phone toolbar is not collapsed by default");
    const tapPhone = async (point, jitter = 0) => {
      await rpc("Input.dispatchTouchEvent", {
        type: "touchStart",
        touchPoints: [{ id: 1, x: point[0], y: point[1], radiusX: 2, radiusY: 2, force: 1 }],
      });
      if (jitter)
        await rpc("Input.dispatchTouchEvent", {
          type: "touchMove",
          touchPoints: [
            { id: 1, x: point[0] + jitter, y: point[1], radiusX: 2, radiusY: 2, force: 1 },
          ],
        });
      await rpc("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await delay(750);
    };
    const handlePoint = () =>
      evaluate(
        "(()=>{const r=handle.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()",
      );
    await evaluate("game.warzoneTrace=[]");
    await tapPhone(await handlePoint());
    if (await evaluate("actions.hidden")) throw Error("Phone toolbar did not expand");
    const initial = await evaluate("[toolbar.offsetLeft,toolbar.offsetTop]");
    const start = await handlePoint();
    await rpc("Input.dispatchTouchEvent", {
      type: "touchStart",
      touchPoints: [{ id: 1, x: start[0], y: start[1], radiusX: 2, radiusY: 2, force: 1 }],
    });
    // In the portrait fallback, local (-180,+80) is visual (-80,-180).
    await rpc("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [
        { id: 1, x: start[0] - 80, y: start[1] - 180, radiusX: 2, radiusY: 2, force: 1 },
      ],
    });
    await rpc("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await delay(500);
    const moved = await evaluate("[toolbar.offsetLeft,toolbar.offsetTop]");
    if (Math.hypot(moved[0] - initial[0], moved[1] - initial[1]) < 80)
      throw Error("Toolbar could not be dragged");
    if (await evaluate("actions.hidden || game.warzoneTrace.length!==0"))
      throw Error("Toolbar drag toggled controls or leaked into the game");
    await tapPhone(await handlePoint());
    if (
      !(await evaluate(
        "actions.hidden && JSON.parse(localStorage.getItem('warzone-toolbar')).collapsed",
      ))
    )
      throw Error("Toolbar collapse was not saved");
    movableToolbar = true;

    const scene = JSON.parse(await readFile("assets/pack/scene.json", "utf8"));
    const scale = 390 / 320,
      w = 844 / scale,
      h = 390 / scale;
    const point = (key, id, origin) => {
      const r = scene.interface.symbols[key].buttons[id];
      return [(origin[0] + r[0] + r[2] / 2) * scale, (origin[1] + r[1] + r[3] / 2) * scale];
    };
    const open = point("main", "inventory_btn", [
      w - scene.interface.symbols.main.size[0] - 5,
      h - 45,
    ]);
    const close = point("inventory", "close_btn", [
      237.5 + w - 480,
      h - scene.interface.symbols.inventory.size[1] + 10.5,
    ]);
    const visualPoint = ([x, y], rotated) => (rotated ? [390 - y, x] : [x, y]);
    for (const rotated of [true, false]) {
      if (!rotated) {
        await rpc("Emulation.setDeviceMetricsOverride", {
          width: 844,
          height: 390,
          screenWidth: 844,
          screenHeight: 390,
          deviceScaleFactor: 3,
          mobile: true,
        });
        await until(
          () =>
            evaluate(
              "!stage.classList.contains('rotated') && canvas.width===2532 && canvas.height===1170",
            ),
          15,
        );
      }
      await tapPhone(visualPoint(open, rotated), 3);
      if ((await evaluate("game.warzoneTrace.at(-1)?.panel")) !== 6)
        throw Error(`Retina inventory tap missed; rotated=${rotated}`);
      await screenshot(rotated ? "phone-portrait-inventory" : "phone-landscape-inventory");
      await tapPhone(visualPoint(close, rotated));
      if ((await evaluate("game.warzoneTrace.at(-1)?.panel")) !== -1)
        throw Error(`Retina inventory close missed; rotated=${rotated}`);
    }
    await screenshot("phone-landscape-retina");
    phoneRetina = true;
  }
  let pinchZoom = false;
  if (process.env.WARZONE_TOUCH === "1") {
    const [x, y] = await evaluate("[innerWidth/2,innerHeight/2]");
    await rpc("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 2 });
    const p = (id, x) => ({ id, x, y, radiusX: 2, radiusY: 2, force: 1 });
    await rpc("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [p(1, x - 40)] });
    await rpc("Input.dispatchTouchEvent", {
      type: "touchStart",
      touchPoints: [p(1, x - 40), p(2, x + 40)],
    });
    await rpc("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [p(1, x - 100), p(2, x + 100)],
    });
    await rpc("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await until(
      () =>
        evaluate(
          `JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'})).state.camera[2]!==${state.state.camera[2]}`,
        ),
      15,
    );
    pinchZoom = true;
    const wallet = await evaluate(
      "JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'})).state.resources",
    );
    if (JSON.stringify(wallet) !== JSON.stringify(state.state.resources))
      throw Error("Pinch accidentally changed resources");
    await rpc("Emulation.setTouchEmulationEnabled", { enabled: false });
  }
  const serviceWorker = await until(
    () => evaluate("navigator.serviceWorker.getRegistration().then(r=>!!r?.active)"),
    60,
  );
  await until(
    () =>
      evaluate(
        "caches.keys().then(async keys=>{for(const k of keys){const c=await caches.open(k);if((await c.keys()).some(r=>new URL(r.url).pathname.endsWith('/game.data')))return true;}return false;})",
      ),
    120,
  );
  await evaluate(
    "(()=>{const f=document.createElement('iframe');f.id='second-writer';f.hidden=true;f.src=location.href;document.body.append(f);})()",
  );
  await until(
    () =>
      evaluate(
        "(()=>{const d=document.querySelector('#second-writer')?.contentDocument;return d?.readyState==='complete'&&!!d.querySelector('#start')})()",
      ),
    30,
  );
  await evaluate(
    "document.querySelector('#second-writer').contentDocument.querySelector('#start').click()",
  );
  await until(
    () =>
      evaluate(
        "document.querySelector('#second-writer').contentDocument.querySelector('#status').textContent.includes('уже открыта')",
      ),
    15,
  );
  if (
    !(await evaluate(
      "document.querySelector('#second-writer').contentDocument.querySelector('#status').classList.contains('error')",
    ))
  )
    throw Error("Second-tab warning has no error styling");
  if (
    (await evaluate(
      "getComputedStyle(document.querySelector('#second-writer').contentDocument.querySelector('#status')).color",
    )) !== "rgb(207, 44, 19)"
  )
    throw Error("Second-tab warning is not red");
  await evaluate(
    "(()=>{const frame=document.querySelector('#second-writer');frame.hidden=false;frame.style.cssText='position:fixed;inset:0;width:100vw;height:100dvh;border:0;z-index:100';return frame.contentDocument.fonts.ready.then(()=>true)})()",
  );
  await screenshot("second-tab-warning");
  await evaluate("document.querySelector('#second-writer').remove()");
  // Reload from the installed cache with transport disabled; saves survive reload.
  await rpc("Network.emulateNetworkConditions", {
    offline: true,
    latency: 0,
    downloadThroughput: 0,
    uploadThroughput: 0,
  });
  await reload();
  await evaluate("document.querySelector('#start').click()");
  await until(() =>
    evaluate(
      "typeof game!=='undefined' && game?.FS && (()=>{try{return !!JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'})).state}catch{return false}})()",
    ),
  );
  const after = await evaluate(
    "JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'}))",
  );
  if (JSON.stringify(after.state.resources) !== JSON.stringify(state.state.resources))
    throw Error("Resources changed on offline reload");
  // Import UI is available before main starts. Test it at that real lifecycle
  // point, instead of loading a second Scene alongside a running game.
  await reload();
  const corrupt = structuredClone(after);
  corrupt.state.resources.realMoney++;
  const encoded = Buffer.from(JSON.stringify(corrupt)).toString("base64");
  await evaluate(
    `importedSave=Uint8Array.from(atob(${JSON.stringify(encoded)}),c=>c.charCodeAt(0));document.querySelector('#start').click();`,
  );
  await until(
    () =>
      evaluate("document.querySelector('#status').textContent.includes('Сохранение повреждено')"),
    120,
  );
  const preserved = await evaluate(
    "JSON.parse(game.FS.readFile('/saves/main/state.json',{encoding:'utf8'}))",
  );
  if (JSON.stringify(preserved.state.resources) !== JSON.stringify(after.state.resources))
    throw Error("Rejected import changed the saved wallet");
  const report = {
    format: "warzone-browser-smoke-v1",
    rendered: true,
    serviceWorker,
    offlineReload: true,
    secondWriterBlocked: true,
    saveFormat: state.format,
    entities: Object.keys(state.state.entities).length,
    resources: state.state.resources,
    nativeImport: !!process.env.WARZONE_IMPORT_SAVE,
    tamperedImportRejected: true,
    pinchZoom,
    fullscreenToggle,
    widePanels,
    phoneRetina,
    movableToolbar,
    loaderFrames: [0, 25, 50, 75, 100],
    staticBackground: posterSize,
    centeredWelcome: true,
    communityIconButtons: true,
    welcomeButtonHover: !phoneMode,
    console: events.filter((e) =>
      ["Runtime.consoleAPICalled", "Runtime.exceptionThrown", "Log.entryAdded"].includes(e.method),
    ),
  };
  await writeFile(join(out, "report.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ ...report, console: report.console.length }));
} catch (error) {
  try {
    await writeFile(
      join(out, "failure-viewport.json"),
      JSON.stringify(
        await evaluate(`({
      native:typeof game==='undefined'?null:game?.warzoneViewport,
      css:[innerWidth,innerHeight],dpr:devicePixelRatio,
      buffer:[canvas.width,canvas.height],canvas:canvas.getBoundingClientRect().toJSON(),
      stage:[stage.clientWidth,stage.clientHeight],rotated:stage.classList.contains('rotated')
    })`),
        null,
        2,
      ),
    );
  } catch {}
  await writeFile(
    join(out, "failure.json"),
    JSON.stringify({ error: String(error), events }, null, 2),
  );
  throw error;
} finally {
  clearInterval(memoryTimer);
  ws?.close();
  browser.kill("SIGTERM");
  await Promise.race([browserExited, delay(5000)]);
  await writeFile(join(out, "chromium.log"), browserLog);
  await rm(profile, { recursive: true, force: true, maxRetries: 3 });
}
