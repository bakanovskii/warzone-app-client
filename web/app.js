const canvas = document.querySelector("#canvas");
const statusText = document.querySelector("#status");
let game,
  running = false,
  importedSave;
const loadingStatus = document.querySelector("#loading-status");
const downloads = { wasm: 0, data: 0 };
const assetVersions = __ASSET_VERSIONS__;
const asset = (name) => `${name}?v=${assetVersions[name]}`;
const resourceCache = "wz-assets-v1";
const loader = document.querySelector("#loading");
const stage = document.querySelector("#game-stage");
const phone = () =>
  matchMedia("(pointer: coarse)").matches && Math.min(innerWidth, innerHeight) <= 600;
const stageSize = () => {
  const rotate =
    phone() &&
    innerHeight > innerWidth &&
    (document.body.classList.contains("launching") || document.body.classList.contains("playing"));
  return rotate ? [innerHeight, innerWidth] : [innerWidth, innerHeight];
};
const fitStage = () => {
  const [width, height] = stageSize();
  stage.classList.toggle("rotated", width !== innerWidth);
  stage.style.width = `${width}px`;
  stage.style.height = `${height}px`;
};
// In portrait Safari the game surface turns, rather than asking iOS for an
// unsupported orientation lock. Convert visual touches back into stage axes.
const stagePoint = (x, y) => (stage.classList.contains("rotated") ? [y, innerWidth - x] : [x, y]);
const canvasPoint = (x, y) => {
  if (!stage.classList.contains("rotated")) return [x, y];
  const [sx, sy] = stagePoint(x, y),
    rect = canvas.getBoundingClientRect();
  return [
    rect.left + (sx / stage.clientWidth) * rect.width,
    rect.top + (sy / stage.clientHeight) * rect.height,
  ];
};
const fitLoader = () => {
  fitStage();
  const [viewWidth, viewHeight] = stageSize();
  const aspect = viewWidth / viewHeight;
  const [layout, width, height] =
    aspect <= 1.4 ? ["ipad", 512, 384] : aspect <= 1.5 ? ["phone", 480, 320] : ["long", 568, 320];
  loader.dataset.layout = layout;
  loader.style.setProperty("--loading-scale", Math.min(viewWidth / width, viewHeight / height));
};
fitLoader();
addEventListener("resize", fitLoader);

const preferLandscape = async () => {
  if (!matchMedia("(pointer: coarse)").matches || !screen.orientation?.lock) return;
  try {
    // Installed PWAs can lock directly; normal Android tabs require fullscreen.
    if (!matchMedia("(display-mode: standalone)").matches && !document.fullscreenElement) {
      await document.documentElement.requestFullscreen?.();
    }
    await screen.orientation.lock("landscape");
  } catch {
    // iOS and unsupported browsers keep control of physical screen orientation.
  }
};
const showProgress = (label, phase, loaded, total) => {
  loadingStatus.textContent = label;
  if (phase && total) downloads[phase] = loaded / total;
  const value = downloads.wasm * 0.08 + downloads.data * 0.92;
  document.querySelector("#progress").value = value;
  document.querySelector("#percent").textContent = `${Math.floor(value * 100)}%`;
};
let scriptPromise;
const loadScript = () =>
  (scriptPromise ||= new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = asset("game.js");
    script.onload = resolve;
    script.onerror = () => reject(Error("Не удалось загрузить клиент"));
    document.head.append(script);
  }));
async function downloadGameFile(name, phase) {
  const url = asset(name);
  let cache, response;
  try {
    cache = await caches.open(resourceCache);
    response = await cache.match(url);
  } catch (error) {
    console.warn("Warzone resource cache:", error);
  }
  const local = !!response;
  let cached = Promise.resolve();
  if (!response) {
    response = await fetch(url, { cache: "force-cache" });
    if (!response.ok) throw Error(`Не удалось загрузить ${name}`);
    // Drain both streams concurrently; do not hold a second complete package
    // in JS memory or download it again when preparing offline mode.
    if (cache)
      cached = cache.put(url, response.clone()).catch((error) => {
        console.warn("Warzone resource cache:", error);
      });
  }
  const total = Number(response.headers.get("Content-Length"));
  const label = local
    ? phase === "wasm"
      ? "Читаем WASM-клиент с устройства…"
      : "Читаем игровые ресурсы с устройства…"
    : phase === "wasm"
      ? "Загружаем WASM-клиент…"
      : "Загружаем игровые ресурсы…";
  if (!response.body) {
    const bytes = new Uint8Array(await response.arrayBuffer());
    await cached;
    downloads[phase] = 1;
    return bytes;
  }
  const reader = response.body.getReader(),
    chunks = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    loaded += value.length;
    showProgress(label, phase, loaded, total);
  }
  const bytes = new Uint8Array(loaded);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  await cached;
  downloads[phase] = 1;
  return bytes;
}
const size = () => {
  const [width, height] = stageSize();
  // Keep the aspect ratio and use Retina pixels, with a bounded GPU surface.
  const ratio = Math.min(devicePixelRatio || 1, 3, 3840 / width, 2160 / height);
  return [Math.max(1, Math.round(width * ratio)), Math.max(1, Math.round(height * ratio))];
};
const fail = (error) => {
  console.error(error);
  const message = String(error?.message || error);
  statusText.textContent = message.includes("уже открыта")
    ? "Игра уже открыта в другой вкладке. Закрой её, чтобы продолжить здесь."
    : `Не удалось запустить игру: ${message}`;
  statusText.classList.add("error");
  const rescue = document.querySelector("#rescue-save");
  rescue.hidden = true;
  try {
    // Even a failed disk commit can leave a useful complete copy in memory.
    rescue.hidden = !game?.FS?.readFile("/saves/main/state.json").length;
  } catch {
    // A missing main file must never be synthesized just to enable export.
  }
  running = false;
  canvas.hidden = true;
  document.querySelector("#toolbar").hidden = true;
  document.querySelector("#start").disabled = false;
  document.querySelector("#restore").disabled = false;
  document.querySelector("#loading").hidden = true;
  document.querySelector("#welcome").hidden = false;
  document.body.classList.remove("launching", "playing");
  fitLoader();
};

async function launch() {
  statusText.textContent = "";
  statusText.classList.remove("error");
  document.querySelector("#rescue-save").hidden = true;
  document.querySelector("#start").disabled = true;
  document.querySelector("#restore").disabled = true;
  document.querySelector("#welcome").hidden = true;
  document.querySelector("#loading").hidden = false;
  document.body.classList.add("launching");
  fitLoader();
  void preferLandscape();
  showProgress("Загружаем WASM-клиент…");
  // Resume audio in the actual user gesture, before downloading the module.
  window.WarzoneAudio.init();
  void window.WarzoneStorage.requestPersistence();
  // One live writer per origin; a second tab must not overwrite a running game.
  const play = async () => {
    let runtimeError, stopRuntime;
    const stopped = new Promise((_, reject) => {
      stopRuntime = reject;
    });
    // Aborts can happen before factory initialization has finished.
    stopped.catch(() => {});
    await loadScript();
    const wasmBinary = await downloadGameFile("game.wasm", "wasm");
    const wasmModule = await WebAssembly.compile(wasmBinary);
    const packageData = await downloadGameFile("game.data", "data");
    const [width, height] = size();
    canvas.width = width;
    canvas.height = height;
    const runtime = createWarzone({
      canvas,
      noInitialRun: true,
      instantiateWasm: (imports, receive) => {
        // Older Chromium limits the synchronous constructor on the main thread.
        WebAssembly.instantiate(wasmModule, imports)
          .then((instance) => receive(instance, wasmModule))
          .catch(stopRuntime);
        return {};
      },
      getPreloadedPackage: () => packageData.buffer,
      locateFile: asset,
      print: (line) => console.log(line),
      printErr: (line) => {
        console.error(line);
        if (line.startsWith("warzone-app-client: ")) {
          runtimeError = Error(line.slice("warzone-app-client: ".length));
          stopRuntime(runtimeError);
        }
      },
      setStatus: (text) => {
        const bytes = text.match(/\((\d+)\/(\d+)\)/);
        if (bytes)
          showProgress("Загружаем игровые ресурсы…", "data", Number(bytes[1]), Number(bytes[2]));
      },
      monitorRunDependencies: (left) => {
        if (!left) showProgress("Подготавливаем игру…");
      },
      onAbort: (error) => stopRuntime(Error(String(error))),
    });
    // A rejected instantiation never calls receive, so the factory could keep waiting.
    game = await Promise.race([runtime, stopped]);
    // Runtime initialization must finish before accessing the filesystem. A
    // failed read rejects launch and releases its lock; no run dependency hangs.
    game.FS.mkdirTree("/saves");
    game.FS.mount(game.IDBFS, {}, "/saves");
    window.WarzoneStorage.configure(game);
    await window.WarzoneStorage.populate(game);
    if (importedSave) {
      game.FS.writeFile("/tmp/import.json", importedSave);
      if (game._wz_validate_import() !== 0)
        throw Error("Сохранение повреждено или относится к другой версии карты");
      const target = "/saves/main/state.json";
      try {
        game.FS.writeFile("/saves/main/import-before.json", game.FS.readFile(target));
      } catch (_) {}
      game.FS.writeFile(target, importedSave);
      // The import is a new checkpoint; keep the previous profile separately,
      // rather than letting an unrelated old backup undo the explicit import.
      game.FS.writeFile("/saves/main/state.bak", importedSave);
      game.FS.writeFile("/saves/main/state.checkpoint.bak", importedSave);
      await new Promise((resolve, reject) =>
        game.FS.syncfs(false, (error) => (error ? reject(error) : resolve())),
      );
    }
    document.querySelector("#welcome").hidden = true;
    canvas.hidden = false;
    running = true;
    canvas.focus();
    showProgress("Открываем карту…", "data", 1, 1);
    // A rotation can happen while the large data package is being loaded.
    const [readyWidth, readyHeight] = size();
    game.setCanvasSize(readyWidth, readyHeight);
    const exitCode = game.callMain([
      "--pack",
      "/assets",
      "--profile",
      "/saves/main",
      "--window-size",
      `${readyWidth}x${readyHeight}`,
    ]);
    if (exitCode)
      throw runtimeError || Error("Игра завершилась с ошибкой. Сохранение не сброшено.");
    // Avoid fetching/caching the large IPA package alongside initial WASM
    // compilation and MEMFS population on memory-constrained devices.
    if ("serviceWorker" in navigator && isSecureContext) {
      setTimeout(() => installOfflineCache(), 5000);
    }
    // Asyncify may return control while the Rust loop is suspended. Retain lock.
    await stopped;
  };
  try {
    if (navigator.locks) {
      await navigator.locks.request("warzone-save-main", { ifAvailable: true }, async (lock) => {
        if (!lock) throw Error("Игра уже открыта в другой вкладке");
        return play();
      });
    } else {
      throw Error(
        "Этот браузер не поддерживает безопасное сохранение. Открой игру в актуальном Safari, Chrome или Firefox.",
      );
    }
  } catch (error) {
    fail(error);
  }
}
document.querySelector("#start").addEventListener("click", launch);
document.querySelector("#restore").onclick = () => document.querySelector("#save-file").click();
document.querySelector("#save-file").onchange = async (event) => {
  const file = event.target.files[0];
  if (!file) return;
  try {
    if (file.size > 16 * 1024 * 1024) throw Error("Сохранение слишком большое");
    const text = await file.text(),
      parsed = JSON.parse(text);
    if (!/^warzone-desktop-save-v[1-8]$/.test(parsed.format) || !parsed.state || !parsed.sha256)
      throw Error("Неверный формат сохранения");
    importedSave = new TextEncoder().encode(text);
    statusText.classList.remove("error");
    statusText.textContent = `Выбрано сохранение: ${file.name}. Нажми «Начать игру».`;
  } catch (error) {
    statusText.textContent = error.message;
    statusText.classList.add("error");
  }
};
document.querySelector("#export").onclick = () => {
  try {
    const bytes = game.FS.readFile("/saves/main/state.json");
    const url = URL.createObjectURL(new Blob([bytes], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = "warzone-save.json";
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 5000);
  } catch (error) {
    alert(`Не удалось скачать сохранение: ${error.message}`);
  }
};
document.querySelector("#rescue-save").onclick = () => document.querySelector("#export").click();
document.querySelector("#fullscreen").onclick = async () => {
  try {
    if (document.fullscreenElement) await document.exitFullscreen();
    else if (document.webkitFullscreenElement) await document.webkitExitFullscreen();
    else if (document.documentElement.requestFullscreen)
      await document.documentElement.requestFullscreen();
    else document.documentElement.webkitRequestFullscreen?.();
  } catch (error) {
    console.warn("Fullscreen:", error);
  }
};
const fullscreenState = () => {
  const active = !!(document.fullscreenElement || document.webkitFullscreenElement);
  const button = document.querySelector("#fullscreen");
  button.setAttribute("aria-pressed", String(active));
  button.setAttribute("aria-label", active ? "Выйти из полноэкранного режима" : "На весь экран");
  button.title = button.getAttribute("aria-label");
  button
    .querySelector("path")
    .setAttribute(
      "d",
      active ? "M3 8h5V3m8 0v5h5M3 16h5v5m8 0v-5h5" : "M8 3H3v5m13-5h5v5M3 16v5h5m8 0h5v-5",
    );
};
document.addEventListener("fullscreenchange", fullscreenState);
document.addEventListener("webkitfullscreenchange", fullscreenState);
const mouse = (type, x, y, buttons) => {
  // SDL's Emscripten button callback reads the cached mouse position, rather
  // than the coordinates on the down/up event. A tap has no preceding move.
  if (type !== "mousemove") mouse("mousemove", x, y, type === "mouseup" ? 1 : 0);
  canvas.dispatchEvent(
    new MouseEvent(type, {
      clientX: x,
      clientY: y,
      button: 0,
      buttons,
      bubbles: true,
      cancelable: true,
    }),
  );
};
for (const type of ["mousedown", "mouseup", "mousemove"]) {
  canvas.addEventListener(
    type,
    (event) => {
      if (!stage.classList.contains("rotated") || !event.isTrusted) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      mouse(type, ...canvasPoint(event.clientX, event.clientY), event.buttons);
    },
    true,
  );
}
const zoom = (amount, x = innerWidth / 2, y = innerHeight / 2) => {
  mouse("mousemove", x, y, 0);
  canvas.dispatchEvent(
    new WheelEvent("wheel", {
      clientX: x,
      clientY: y,
      deltaY: amount,
      bubbles: true,
      cancelable: true,
    }),
  );
};
document.querySelector("#zoom-in").onclick = () => zoom(-100);
document.querySelector("#zoom-out").onclick = () => zoom(100);
canvas.addEventListener(
  "wheel",
  (event) => {
    if (!stage.classList.contains("rotated") || !event.isTrusted) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    zoom(event.deltaY, ...canvasPoint(event.clientX, event.clientY));
  },
  { capture: true, passive: false },
);
let resizeFrame,
  lastRotation = stage.classList.contains("rotated");
const resizeGame = () => {
  cancelAnimationFrame(resizeFrame);
  resizeFrame = requestAnimationFrame(() => {
    const wasRotated = lastRotation;
    fitLoader();
    lastRotation = stage.classList.contains("rotated");
    positionToolbar();
    if (!game) return;
    const [width, height] = size();
    if (canvas.width !== width || canvas.height !== height || wasRotated !== lastRotation) {
      game._wz_cancel_pointer();
      dragging = false;
      lastDistance = 0;
      game.setCanvasSize(width, height);
    }
  });
};
addEventListener("resize", resizeGame);
visualViewport?.addEventListener("resize", resizeGame);
screen.orientation?.addEventListener("change", resizeGame);

const toolbar = document.querySelector("#toolbar");
const handle = document.querySelector("#toolbar-handle");
const actions = document.querySelector("#toolbar-actions");
let toolbarPrefs = {};
try {
  toolbarPrefs = JSON.parse(localStorage.getItem("warzone-toolbar") || "{}");
} catch {}
if (!toolbarPrefs || typeof toolbarPrefs !== "object") toolbarPrefs = {};
let toolbarCollapsed =
  typeof toolbarPrefs.collapsed === "boolean" ? toolbarPrefs.collapsed : phone();
const toolbarBounds = () => {
  const css = getComputedStyle(document.querySelector("#stage-safe-area"));
  const inset = (name) => parseFloat(css[name]) || 8;
  const left = inset("left"),
    top = inset("top");
  return [
    left,
    top,
    Math.max(left, stage.clientWidth - toolbar.offsetWidth - inset("right")),
    Math.max(top, stage.clientHeight - toolbar.offsetHeight - inset("bottom")),
  ];
};
function positionToolbar() {
  actions.hidden = toolbarCollapsed;
  handle.setAttribute("aria-expanded", String(!toolbarCollapsed));
  handle.setAttribute(
    "aria-label",
    toolbarCollapsed
      ? "Показать панель; потяни для перемещения"
      : "Скрыть панель; потяни для перемещения",
  );
  const [left, top, right, bottom] = toolbarBounds();
  const fraction = (n, fallback) => (Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : fallback);
  toolbar.style.left = `${left + (right - left) * fraction(toolbarPrefs.x, 1)}px`;
  toolbar.style.top = `${top + (bottom - top) * fraction(toolbarPrefs.y, Math.min(1, 72 / Math.max(1, bottom - top)))}px`;
}
const saveToolbar = () => {
  toolbarPrefs.collapsed = toolbarCollapsed;
  try {
    localStorage.setItem("warzone-toolbar", JSON.stringify(toolbarPrefs));
  } catch {}
};
let toolbarDrag,
  toolbarMoved = false;
handle.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  toolbarMoved = false;
  toolbarDrag = {
    point: stagePoint(event.clientX, event.clientY),
    x: toolbar.offsetLeft,
    y: toolbar.offsetTop,
  };
  handle.setPointerCapture(event.pointerId);
});
handle.addEventListener("pointermove", (event) => {
  if (!toolbarDrag) return;
  const point = stagePoint(event.clientX, event.clientY);
  const dx = point[0] - toolbarDrag.point[0],
    dy = point[1] - toolbarDrag.point[1];
  if (!toolbarMoved && Math.hypot(dx, dy) < 6) return;
  toolbarMoved = true;
  const [left, top, right, bottom] = toolbarBounds();
  const x = Math.min(right, Math.max(left, toolbarDrag.x + dx));
  const y = Math.min(bottom, Math.max(top, toolbarDrag.y + dy));
  toolbarPrefs.x = (x - left) / Math.max(1, right - left);
  toolbarPrefs.y = (y - top) / Math.max(1, bottom - top);
  positionToolbar();
});
for (const type of ["pointerup", "pointercancel"])
  handle.addEventListener(type, () => {
    if (toolbarDrag) saveToolbar();
    toolbarDrag = null;
  });
handle.addEventListener("click", () => {
  if (toolbarMoved) {
    toolbarMoved = false;
    return;
  }
  toolbarCollapsed = !toolbarCollapsed;
  positionToolbar();
  saveToolbar();
});
// Toolbar gestures must never become a map click or a deployment drag.
for (const type of [
  "mousedown",
  "mouseup",
  "mousemove",
  "pointerdown",
  "pointerup",
  "pointermove",
  "touchstart",
  "touchmove",
  "touchend",
  "wheel",
])
  toolbar.addEventListener(type, (event) => event.stopPropagation());
// A single finger taps/drags; two fingers zoom. Cancel captures before pinch,
// preventing an accidental placement, purchase or selection on the first finger.
let lastDistance = 0,
  dragging = false;
const gap = (list) =>
  Math.hypot(list[0].clientX - list[1].clientX, list[0].clientY - list[1].clientY);
for (const name of ["touchstart", "touchmove", "touchend", "touchcancel"]) {
  canvas.addEventListener(
    name,
    (event) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!running) return;
      const touches = event.touches;
      if (touches.length >= 2) {
        if (dragging) {
          game._wz_cancel_pointer();
          const p = touches[0];
          mouse("mouseup", ...canvasPoint(p.clientX, p.clientY), 0);
          dragging = false;
        }
        const distance = gap(touches);
        if (lastDistance && name === "touchmove")
          zoom(
            -Math.log(distance / lastDistance) * 500,
            ...canvasPoint(
              (touches[0].clientX + touches[1].clientX) / 2,
              (touches[0].clientY + touches[1].clientY) / 2,
            ),
          );
        lastDistance = distance;
        return;
      }
      if (lastDistance) {
        lastDistance = 0;
        return;
      }
      if (touches.length === 1) {
        const p = touches[0];
        if (name === "touchstart") {
          dragging = true;
          mouse("mousedown", ...canvasPoint(p.clientX, p.clientY), 1);
        } else if (name === "touchmove" && dragging)
          mouse("mousemove", ...canvasPoint(p.clientX, p.clientY), 1);
      } else if (dragging) {
        const p = event.changedTouches[0];
        if (name === "touchcancel") game._wz_cancel_pointer();
        mouse("mouseup", ...canvasPoint(p.clientX, p.clientY), 0);
        dragging = false;
      }
    },
    { capture: true, passive: false },
  );
}
addEventListener(
  "warzone-frame",
  () => {
    document.querySelector("#loading").hidden = true;
    document.querySelector("#toolbar").hidden = false;
    document.body.classList.remove("launching");
    document.body.classList.add("playing");
    fitLoader();
    positionToolbar();
    if (game?.warzoneRecoveredSave) {
      game.warzoneRecoveredSave = false;
      setTimeout(() => {
        alert(
          "Сохранение восстановлено из резервной копии. Последние действия могли не сохраниться.",
        );
      }, 0);
    }
  },
  { once: true },
);
const offlineLabel = document.querySelector("#offline-status");
async function offlineState() {
  if (!isSecureContext || !("serviceWorker" in navigator)) {
    offlineLabel.textContent = "Для офлайн-режима и установки нужен HTTPS или localhost.";
    return;
  }
  const registration = await navigator.serviceWorker.getRegistration();
  registration?.active?.postMessage({ type: "offline-status" });
}
async function installOfflineCache() {
  try {
    const registration = await navigator.serviceWorker.register("sw.js");
    // A previously active worker is not reinstalled when the script is
    // unchanged. Explicitly repair evicted/missing cache entries in that case.
    if (!registration.installing && !registration.waiting)
      registration.active?.postMessage({ type: "offline-prepare" });
    await offlineState();
  } catch (error) {
    offlineLabel.textContent =
      "Не удалось подготовить офлайн-режим. Прогресс игры сохраняется отдельно.";
    console.error(error);
  }
}
if ("serviceWorker" in navigator) {
  navigator.serviceWorker.addEventListener("message", ({ data }) => {
    if (data?.type === "offline-ready")
      offlineLabel.textContent = "Офлайн-режим готов. Можно играть без интернета.";
    if (data?.type === "offline-progress")
      offlineLabel.textContent = `Готовим офлайн-режим: ${data.done} из ${data.total} файлов.`;
    if (data?.type === "offline-error")
      offlineLabel.textContent =
        "Ресурсы для офлайн-режима загружены не полностью. Нужен повторный запуск с интернетом.";
  });
  offlineState().catch(console.error);
}
let installPrompt;
addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  installPrompt = event;
  document.querySelector("#install").hidden = false;
});
document.querySelector("#install").onclick = async () => {
  if (!installPrompt) return;
  await installPrompt.prompt();
  const choice = await installPrompt.userChoice;
  installPrompt = null;
  if (choice.outcome === "accepted") navigator.storage?.persist?.().catch(console.error);
  document.querySelector("#install").hidden = true;
};
