import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../web/sw.js", import.meta.url), "utf8");
const origin = "https://game.test/";
const url = (name, hash = "a") => `${name}?v=${hash.repeat(20)}`;
const packageURL = url("game.data");
const documentURL = url("index.html");

class CacheStorage {
  stores = new Map();
  failWrites = false;
  async open(name) {
    if (!this.stores.has(name)) this.stores.set(name, new Map());
    const entries = this.stores.get(name);
    const key = (request) => new URL(request.url || request, origin).href;
    return {
      match: async (request) => entries.get(key(request))?.clone(),
      put: async (request, response) => {
        if (this.failWrites) throw Error("QuotaExceededError");
        entries.set(
          key(request),
          new Response(await response.arrayBuffer(), { status: response.status }),
        );
      },
      delete: async (request) => entries.delete(key(request)),
      keys: async () => [...entries.keys()].map((value) => new Request(value)),
    };
  }
  async keys() {
    return [...this.stores.keys()];
  }
  async delete(name) {
    return this.stores.delete(name);
  }
  async match(request) {
    for (const name of this.stores.keys()) {
      const response = await (await this.open(name)).match(request);
      if (response) return response;
    }
  }
}

function worker({ caches, files = ["./", documentURL, packageURL], tag = "a".repeat(20), fetch }) {
  const handlers = new Map();
  const messages = [];
  const location = new URL(`${origin}sw.js`);
  vm.runInNewContext(
    source.replace("__BUILD_HASH__", tag).replace("__PRECACHE_FILES__", JSON.stringify(files)),
    {
      caches,
      fetch,
      URL,
      Response,
      AbortController,
      setTimeout,
      clearTimeout,
      location,
      self: {
        location,
        clients: {
          matchAll: async () => [{ postMessage: (message) => messages.push(message) }],
          claim: async () => {},
        },
        addEventListener: (name, handler) => handlers.set(name, handler),
      },
    },
  );
  return {
    messages,
    async dispatch(name, properties = {}) {
      const promises = [];
      let response;
      handlers.get(name)({
        ...properties,
        waitUntil: (promise) => promises.push(promise),
        respondWith: (promise) => {
          response = promise;
        },
      });
      const result = await response;
      await Promise.all(promises);
      return result;
    },
  };
}

test("a new shell release reuses the existing large package and prunes obsolete versions", async () => {
  const caches = new CacheStorage();
  const cache = await caches.open("wz-assets-v1");
  await cache.put(packageURL, new Response("already downloaded package"));
  const obsolete = url("game.wasm", "b");
  await cache.put(obsolete, new Response("old wasm"));
  await caches.open("warzone-old");
  const requests = [];
  const sw = worker({
    caches,
    fetch: async (request) => {
      requests.push(request);
      return new Response("new shell");
    },
  });
  await sw.dispatch("install");
  await sw.dispatch("activate");
  assert.equal(requests.filter((request) => request.includes("game.data")).length, 0);
  assert.equal(await (await cache.match(packageURL)).text(), "already downloaded package");
  assert.equal(await cache.match(obsolete), undefined);
  assert.equal((await caches.keys()).includes("warzone-old"), false);
});

test("unchanged release probes only the manifest and returns the installed page", async () => {
  const caches = new CacheStorage();
  await (await caches.open(`warzone-${"a".repeat(20)}`)).put("./", new Response("installed page"));
  const requests = [];
  const sw = worker({
    caches,
    fetch: async (request, options) => {
      requests.push(String(request));
      assert.equal(options.cache, "no-store");
      return Response.json({ build: "a".repeat(20) });
    },
  });
  const page = await sw.dispatch("fetch", {
    request: { method: "GET", mode: "navigate", url: origin },
  });
  assert.equal(await page.text(), "installed page");
  assert.deepEqual(requests, [`${origin}version.json`]);
});

test("a new release fetches fresh HTML, without downloading the game package", async () => {
  const caches = new CacheStorage();
  await (await caches.open(`warzone-${"a".repeat(20)}`)).put("./", new Response("installed page"));
  const requests = [];
  const sw = worker({
    caches,
    fetch: async (request) => {
      requests.push(request.url || String(request));
      if (String(request).endsWith("version.json")) return Response.json({ build: "b".repeat(20) });
      return new Response("updated page");
    },
  });
  const page = await sw.dispatch("fetch", {
    request: { method: "GET", mode: "navigate", url: `${origin}?campaign=1` },
  });
  assert.equal(await page.text(), "updated page");
  assert.deepEqual(requests, [`${origin}version.json`, `${origin}?campaign=1`]);
});

test("failed, invalid, and stalled probes retain the installed page", async () => {
  const caches = new CacheStorage();
  await (await caches.open(`warzone-${"a".repeat(20)}`)).put("./", new Response("installed page"));
  for (const fetch of [
    async () => {
      throw Error("offline");
    },
    async () => new Response("missing", { status: 404 }),
    async () => new Response("invalid JSON"),
    async () => Response.json(null),
    async () => Response.json({ build: "" }),
    (_, { signal }) =>
      new Promise((_, reject) => signal.addEventListener("abort", () => reject(Error("timeout")))),
  ]) {
    const sw = worker({ caches, fetch });
    const page = await sw.dispatch("fetch", {
      request: { method: "GET", mode: "navigate", url: origin },
    });
    assert.equal(await page.text(), "installed page");
  }
});

test("incomplete server updates do not replace a usable cached page", async () => {
  const caches = new CacheStorage();
  await (await caches.open(`warzone-${"a".repeat(20)}`)).put("./", new Response("installed page"));
  const sw = worker({
    caches,
    fetch: async (request) => {
      if (String(request).endsWith("version.json")) return Response.json({ build: "b".repeat(20) });
      return new Response("unavailable", { status: 503 });
    },
  });
  const page = await sw.dispatch("fetch", {
    request: { method: "GET", mode: "navigate", url: origin },
  });
  assert.equal(await page.text(), "installed page");
});

test("an evicted cached page can still be fetched online", async () => {
  const requests = [];
  const sw = worker({
    caches: new CacheStorage(),
    fetch: async (request) => {
      requests.push(request.url);
      return new Response("first page");
    },
  });
  const page = await sw.dispatch("fetch", {
    request: { method: "GET", mode: "navigate", url: origin },
  });
  assert.equal(await page.text(), "first page");
  assert.deepEqual(requests, [origin]);
});

test("unavailable cache storage still permits online navigation", async () => {
  const sw = worker({
    caches: {
      open: async () => {
        throw Error("Storage unavailable");
      },
    },
    fetch: async () => new Response("online page"),
  });
  const page = await sw.dispatch("fetch", {
    request: { method: "GET", mode: "navigate", url: origin },
  });
  assert.equal(await page.text(), "online page");
});

test("interrupted installation keeps completed downloads and retries only missing files", async () => {
  const caches = new CacheStorage();
  const extra = url("audio.js");
  const requests = [];
  let interrupted = true;
  const fetch = async (request) => {
    requests.push(request);
    if (request.includes("audio.js") && interrupted) throw Error("connection interrupted");
    return new Response("asset");
  };
  const files = ["./", documentURL, packageURL, extra];
  await assert.rejects(worker({ caches, files, fetch }).dispatch("install"));
  assert.ok(await (await caches.open("wz-assets-v1")).match(packageURL));
  interrupted = false;
  const sw = worker({ caches, files, fetch });
  await sw.dispatch("install");
  assert.equal(requests.filter((request) => request.includes("game.data")).length, 1);
  const replies = [];
  await sw.dispatch("message", {
    data: { type: "offline-status" },
    source: { postMessage: (data) => replies.push(data.type) },
  });
  assert.deepEqual(replies, ["offline-ready"]);
  await (await caches.open("wz-assets-v1")).delete(packageURL);
  await sw.dispatch("message", {
    data: { type: "offline-status" },
    source: { postMessage: (data) => replies.push(data.type) },
  });
  assert.equal(replies.at(-1), "offline-error");
  await sw.dispatch("message", { data: { type: "offline-prepare" } });
  assert.ok(await (await caches.open("wz-assets-v1")).match(packageURL));
  assert.equal(requests.filter((request) => request.includes("index.html")).length, 1);
  assert.equal(requests.filter((request) => request.includes("game.data")).length, 2);
});

test("offline navigation and files work; cache quota failure still permits online play", async () => {
  const caches = new CacheStorage();
  let offline = false;
  const sw = worker({
    caches,
    fetch: async () => {
      if (offline) throw Error("offline");
      return new Response("valid file");
    },
  });
  await sw.dispatch("install");
  offline = true;
  const page = await sw.dispatch("fetch", {
    request: { method: "GET", mode: "navigate", url: origin },
  });
  assert.equal(await page.text(), "valid file");
  const data = await sw.dispatch("fetch", { request: new Request(new URL(packageURL, origin)) });
  assert.equal(await data.text(), "valid file");
  offline = false;
  caches.failWrites = true;
  const live = await sw.dispatch("fetch", {
    request: new Request(new URL(url("game.wasm", "c"), origin)),
  });
  assert.equal(await live.text(), "valid file");
  assert.equal(sw.messages.at(-1).type, "offline-error");
  // Checking an intact offline installation should not need extra disk space.
  await sw.dispatch("message", { data: { type: "offline-prepare" } });
  assert.equal(sw.messages.at(-1).type, "offline-ready");
});
