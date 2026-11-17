// Generated from actual file content hashes. Unchanged packages survive releases.
const CACHE = "warzone-__BUILD_HASH__";
const ASSETS = "wz-assets-v1";
const FILES = __PRECACHE_FILES__;
const READY = "./offline-ready";
const pending = new Map();
async function navigate(request) {
  let cached;
  try {
    cached = await (await caches.open(CACHE)).match("./");
  } catch {
    // Disabled/unreadable storage must still allow an online launch.
  }
  if (!cached) return fetch(request);
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 2000);
  try {
    // A missing/unchanged manifest must not invalidate an installed game. Bound
    // the complete check so an unreachable server cannot stall offline startup.
    const probe = await fetch(new URL("version.json", self.location.href), {
      cache: "no-store",
      signal: controller.signal,
    });
    if (!probe.ok) return cached;
    const version = await probe.json();
    if (!/^[a-f0-9]{20}$/.test(version?.build || "") || `warzone-${version.build}` === CACHE)
      return cached;
    const response = await fetch(request, { cache: "no-store", signal: controller.signal });
    if (response.ok) return response;
  } catch {
    // Offline, invalid manifest, timeout, or an incomplete server deployment.
  } finally {
    clearTimeout(timeout);
  }
  return cached;
}
async function broadcast(data) {
  for (const client of await self.clients.matchAll({ includeUncontrolled: true }))
    client.postMessage(data);
}
async function resource(path) {
  const key = new URL(typeof path === "string" ? path : path.url, self.location.href).href;
  if (pending.has(key)) return (await pending.get(key)).clone();
  const load = (async () => {
    const cache = await caches.open(ASSETS);
    const cached = await cache.match(key);
    if (cached) return cached;
    // Also reuse resources from the preceding cache layout, when keys match.
    const response = (await caches.match(key)) || (await fetch(key, { cache: "force-cache" }));
    if (!response.ok) throw Error(`Could not cache ${key}`);
    await cache.put(key, response);
    return cache.match(key);
  })();
  pending.set(key, load);
  try {
    return (await load).clone();
  } finally {
    pending.delete(key);
  }
}
async function serveResource(request, event) {
  try {
    const cache = await caches.open(ASSETS);
    const cached = await cache.match(request);
    if (cached) return cached;
    if (pending.has(request.url)) return resource(request);
    const response = await fetch(request);
    if (response.ok)
      event.waitUntil(
        cache.put(request, response.clone()).catch(async () => {
          await broadcast({ type: "offline-error" });
        }),
      );
    return response;
  } catch {
    // A full/disabled cache must not block online play.
    return fetch(request);
  }
}
let preparing;
function prepareOffline() {
  if (preparing) return preparing;
  preparing = (async () => {
    const cache = await caches.open(CACHE);
    let done = 0;
    try {
      // Keep completed content-addressed files on interrupted installation.
      // Sequential puts bound network/memory use on phones.
      for (const path of FILES) {
        if (path === "./") {
          if (!(await cache.match(path))) {
            const response = await resource(FILES.find((file) => file.startsWith("index.html?")));
            await cache.put(path, response);
          }
        } else {
          await resource(path);
        }
        await broadcast({ type: "offline-progress", done: ++done, total: FILES.length });
      }
      if (!(await cache.match(READY))) await cache.put(READY, new Response("ready"));
      await broadcast({ type: "offline-ready" });
      // No skipWaiting: an update cannot replace a live game's worker.
    } catch (error) {
      await cache.delete(READY);
      await broadcast({ type: "offline-error" });
      throw error;
    }
  })().finally(() => {
    preparing = null;
  });
  return preparing;
}
self.addEventListener("install", (event) => event.waitUntil(prepareOffline()));
self.addEventListener("activate", (event) =>
  event.waitUntil(
    (async () => {
      for (const name of await caches.keys())
        if (name.startsWith("warzone-") && name !== CACHE) await caches.delete(name);
      const assets = await caches.open(ASSETS);
      const keep = new Set(FILES.slice(1).map((path) => new URL(path, self.location.href).href));
      for (const request of await assets.keys())
        if (!keep.has(request.url)) await assets.delete(request);
      await self.clients.claim();
    })(),
  ),
);
self.addEventListener("message", (event) => {
  if (event.data?.type === "offline-prepare") event.waitUntil(prepareOffline().catch(() => {}));
  if (event.data?.type === "offline-status")
    event.waitUntil(
      (async () => {
        const cache = await caches.open(CACHE);
        const assets = await caches.open(ASSETS);
        let ready = !!((await cache.match(READY)) && (await cache.match("./")));
        // Verify entries instead of trusting a leftover completion marker.
        for (const path of FILES.slice(1))
          if (!(await assets.match(path))) {
            ready = false;
            break;
          }
        event.source?.postMessage({ type: ready ? "offline-ready" : "offline-error" });
      })(),
    );
});
self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== "GET" || url.origin !== location.origin) return;
  const root = new URL("./", self.location.href).pathname;
  if (
    event.request.mode === "navigate" &&
    (url.pathname === root || url.pathname === `${root}index.html`)
  ) {
    event.respondWith(navigate(event.request));
  } else if (
    url.pathname.startsWith(root) &&
    /^[a-f0-9]{20}$/.test(url.searchParams.get("v") || "")
  ) {
    event.respondWith(serveResource(event.request, event));
  }
});
