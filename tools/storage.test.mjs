import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../web/storage.js", import.meta.url), "utf8");
function storage(navigator = {}) {
  const window = {};
  vm.runInNewContext(source, {
    window,
    navigator,
    console,
    TypeError,
    ArrayBuffer,
    Uint8Array,
    Date,
  });
  return window.WarzoneStorage;
}

test("only the game's save database requests strict durability, including subsequent commits", async () => {
  const calls = [];
  const db = {
    transaction: (...args) => {
      calls.push(args);
      return {};
    },
  };
  const module = { IDBFS: { getDB: (_name, callback) => callback(null, db) } };
  storage().configure(module);
  for (let i = 0; i < 2; i++) {
    await new Promise((resolve) => module.IDBFS.getDB("/saves", resolve));
    db.transaction("FILE_DATA", "readwrite");
  }
  assert.equal(calls.length, 2);
  assert.ok(calls.every((call) => call[2].durability === "strict"));
  db.transaction("FILE_DATA", "readonly");
  assert.equal(calls.at(-1)[2], undefined);
});

test("load signed IDBFS byte arrays and preserve null records without trusting the timestamp index", async () => {
  const original = new Uint8Array([123, 34, 207, 159, 34, 125]);
  const records = [
    ["/saves/main", null],
    [
      "/saves/main/state.bak",
      { mode: 33188, contents: new Int8Array(original.buffer), timestamp: new Date() },
    ],
    ["/saves/main/state.json", null],
  ];
  const files = new Map();
  const db = {
    transaction: () => {
      const transaction = {
        objectStore: () => ({
          openCursor: () => {
            const request = {};
            let index = 0;
            const advance = () =>
              queueMicrotask(() => {
                const record = records[index++];
                request.result = record
                  ? { key: record[0], value: record[1], continue: advance }
                  : null;
                request.onsuccess();
                if (!record) transaction.oncomplete();
              });
            advance();
            return request;
          },
        }),
      };
      return transaction;
    },
  };
  const module = {
    IDBFS: { getDB: (_name, callback) => callback(null, db) },
    FS: {
      mkdirTree: () => {},
      isDir: (mode) => (mode & 61440) === 16384,
      utime: () => {},
      writeFile: (path, bytes) => files.set(path, bytes),
      analyzePath: (path) => ({ exists: files.has(path) }),
    },
  };
  await storage().populate(module);
  assert.deepEqual(files.get("/saves/main/state.bak"), original);
  assert.equal(files.get("/saves/main/state.json").length, 0);
  assert.equal(JSON.parse(files.get("/saves/main/storage-damaged.json"))[0].entry, null);
});

test("older browsers fall back, but write/storage failures are not swallowed", async () => {
  let oldBrowser = true;
  const failure = Error("storage unavailable");
  const db = {
    transaction: (_stores, _mode, options) => {
      if (!oldBrowser) throw failure;
      if (options) throw new TypeError("options unsupported");
      return "legacy transaction";
    },
  };
  const module = { IDBFS: { getDB: (_name, callback) => callback(null, db) } };
  storage().configure(module);
  await new Promise((resolve) => module.IDBFS.getDB("/saves", resolve));
  assert.equal(db.transaction("FILE_DATA", "readwrite"), "legacy transaction");
  oldBrowser = false;
  assert.throws(
    () => db.transaction("FILE_DATA", "readwrite"),
    (error) => error === failure,
  );
});

test("request persistent storage for ordinary players, not only PWA installation", async () => {
  let requested = 0;
  await storage({
    storage: {
      persisted: async () => false,
      persist: async () => {
        requested++;
        return false;
      },
    },
  }).requestPersistence();
  assert.equal(requested, 1);
  await storage({
    storage: {
      persisted: async () => true,
      persist: async () => {
        requested++;
      },
    },
  }).requestPersistence();
  assert.equal(requested, 1);
  await storage().requestPersistence();
});

test("invalid timestamp metadata cannot break the next atomic sync", async () => {
  const module = {
    IDBFS: {
      getDB: () => {},
      getRemoteSet: (_mount, callback) =>
        callback(null, {
          entries: {
            "/saves/main/state.json": { timestamp: 42 },
            "/saves/main/state.bak": { timestamp: new Date(123) },
          },
        }),
    },
  };
  storage().configure(module);
  const remote = await new Promise((resolve) =>
    module.IDBFS.getRemoteSet({}, (_error, value) => resolve(value)),
  );
  assert.equal(remote.entries["/saves/main/state.json"].timestamp.getTime(), 0);
  assert.equal(remote.entries["/saves/main/state.bak"].timestamp.getTime(), 123);
});
