// Configure only the game's IDBFS database, leaving other site databases alone.
window.WarzoneStorage = {
  configure(module) {
    const getDB = module.IDBFS.getDB;
    const getRemoteSet = module.IDBFS.getRemoteSet;
    if (getRemoteSet)
      module.IDBFS.getRemoteSet = (mount, callback) =>
        getRemoteSet(mount, (error, remote) => {
          if (!error)
            for (const entry of Object.values(remote.entries))
              if (!(entry.timestamp instanceof Date)) entry.timestamp = new Date(0);
          callback(error, remote);
        });
    const configured = new WeakSet();
    module.IDBFS.getDB = (name, callback) =>
      getDB(name, (error, db) => {
        if (!error && !configured.has(db)) {
          const transaction = db.transaction.bind(db);
          db.transaction = (stores, mode, options) => {
            if (mode === "readwrite") {
              try {
                // Completion should wait for durable storage, rather than the
                // browser's default relaxed write-behind transaction.
                return transaction(stores, mode, { ...options, durability: "strict" });
              } catch (failure) {
                if (!(failure instanceof TypeError)) throw failure;
                // Older WebKit versions do not accept the options argument.
              }
            }
            return transaction(stores, mode, options);
          };
          configured.add(db);
        }
        callback(error, db);
      });
  },
  async populate(module) {
    const db = await new Promise((resolve, reject) =>
      module.IDBFS.getDB("/saves", (error, value) => (error ? reject(error) : resolve(value))),
    );
    // Read the store itself: its timestamp index can omit damaged records.
    // Rebuild parent directories before files instead of dereferencing null
    // directory metadata inside IDBFS's generic reconciliation routine.
    const records = await new Promise((resolve, reject) => {
      const entries = [];
      const transaction = db.transaction("FILE_DATA", "readonly");
      const request = transaction.objectStore("FILE_DATA").openCursor();
      request.onsuccess = () => {
        const cursor = request.result;
        if (cursor) {
          entries.push([cursor.key, cursor.value]);
          cursor.continue();
        }
      };
      transaction.oncomplete = () => resolve(entries);
      transaction.onerror = transaction.onabort = () => reject(transaction.error);
    });
    const repairs = [];
    const { FS } = module;
    FS.mkdirTree("/saves/main");
    for (const [path, entry] of records.sort(([a], [b]) => String(a).localeCompare(String(b)))) {
      if (path === "/saves/main" || path === "/saves") continue;
      if (
        typeof path !== "string" ||
        !path.startsWith("/saves/main/") ||
        path.split("/").some((part) => part === ".." || part === "." || part.includes("\0"))
      )
        throw Error("Неизвестная структура сохранения. Данные оставлены без изменений.");
      const bytes = entry?.contents;
      if (ArrayBuffer.isView(bytes) || bytes instanceof ArrayBuffer || Array.isArray(bytes)) {
        if (bytes.byteLength > 16 * 1024 * 1024 || bytes.length > 16 * 1024 * 1024)
          throw Error("Файл сохранения превышает допустимый размер. Данные не изменены.");
        FS.mkdirTree(path.slice(0, path.lastIndexOf("/")));
        const content = ArrayBuffer.isView(bytes)
          ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength)
          : new Uint8Array(bytes);
        FS.writeFile(path, content);
        if (entry.timestamp instanceof Date && Number.isFinite(entry.timestamp.getTime()))
          FS.utime(path, entry.timestamp, entry.timestamp);
      } else if (entry && FS.isDir(entry.mode)) {
        FS.mkdirTree(path);
      } else {
        repairs.push({ path, entry });
        FS.mkdirTree(path.slice(0, path.lastIndexOf("/")));
        // Keep a damaged/missing file distinguishable from a fresh profile.
        // Rust will verify compatible backups before allowing any new write.
        FS.writeFile(path, new Uint8Array());
      }
    }
    if (records.length && !FS.analyzePath("/saves/main/state.json").exists) {
      FS.writeFile("/saves/main/state.json", new Uint8Array());
    }
    if (repairs.length) FS.writeFile("/saves/main/storage-damaged.json", JSON.stringify(repairs));
  },
  async requestPersistence() {
    try {
      if (navigator.storage?.persist && !(await navigator.storage.persisted()))
        await navigator.storage.persist();
    } catch (error) {
      // A denied request must not prevent playing or saving normally.
      console.warn("Warzone persistent storage:", error);
    }
  },
};
