import { getBrowserApi, isGecko, nativeAllowed } from "./browser-api.js";
import { HOST } from "./shared.js";
const MAX_OUTBOX_BYTES = 8 * 1024 * 1024;

export class NativeBridge {
  #port;
  #pending = new Map();
  get connected() {
    return !!this.#port;
  }
  disconnect() {
    const port = this.#port;
    this.#port = undefined;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error("Desktop connection closed"));
    }
    this.#pending.clear();
    port?.disconnect();
  }
  async request(command, payload, requestId = crypto.randomUUID()) {
    if (isGecko() && !(await nativeAllowed()))
      throw new Error(
        "Enable local Flux data transfer in the extension settings",
      );
    if (!this.#port) {
      this.#port = getBrowserApi().runtime.connectNative(HOST);
      const port = this.#port;
      port.onMessage.addListener((message) => {
        if (this.#port !== port) return;
        const pending = this.#pending.get(message.request_id);
        if (!pending) return;
        clearTimeout(pending.timer);
        this.#pending.delete(message.request_id);
        if (message.version !== 1)
          pending.reject(new Error("Unsupported native protocol"));
        else if (message.ok) pending.resolve(message.result);
        else
          pending.reject(new Error(message.error || "Native request failed"));
      });
      port.onDisconnect.addListener(() => {
        const error =
          port.error?.message ||
          getBrowserApi().runtime.lastError?.message ||
          "Desktop connection closed";
        if (this.#port !== port) return;
        this.#port = undefined;
        for (const pending of this.#pending.values()) {
          clearTimeout(pending.timer);
          pending.reject(new Error(error));
        }
        this.#pending.clear();
      });
      this.onConnected?.();
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => {
          this.#pending.delete(requestId);
          reject(new Error("Desktop request timed out"));
        },
        command === "resolve" || command === "resolve_collection"
          ? 125000
          : command === "inspect"
            ? 55000
            : command === "resolve_manifest"
              ? 25000
              : 15000,
      );
      this.#pending.set(requestId, { resolve, reject, timer });
      try {
        this.#port.postMessage({
          version: 1,
          request_id: requestId,
          command,
          ...(payload === undefined ? {} : { payload }),
        });
      } catch (error) {
        clearTimeout(timer);
        this.#pending.delete(requestId);
        reject(error);
      }
    });
  }
}

/** Storage writes are serialized. A task UUID is persisted before any native message. */
export class DurableOutbox {
  #gate = Promise.resolve();
  #pumping;
  #confirmations = new Map();
  constructor(
    bridge,
    onResult = async () => {},
    prepare = async (entry) => entry.input,
    command = "add",
  ) {
    this.bridge = bridge;
    this.onResult = onResult;
    this.prepare = prepare;
    this.command = command;
  }
  mutate(callback) {
    const next = this.#gate.then(async () => {
      const { outbox = [] } = await getBrowserApi().storage.local.get("outbox");
      const result = await callback(outbox);
      if (
        new TextEncoder().encode(JSON.stringify(outbox)).byteLength >
        MAX_OUTBOX_BYTES
      )
        throw new Error("Transfer queue metadata exceeds eight MiB");
      await getBrowserApi().storage.local.set({ outbox });
      return result;
    });
    this.#gate = next.catch(() => {});
    return next;
  }
  async enqueue(input, downloadId, context = {}, startPump = true) {
    const id = crypto.randomUUID();
    const publicInput = structuredClone(input);
    delete publicInput.credentials;
    await this.mutate((entries) => {
      if (entries.length >= 500) throw new Error("Transfer queue is full");
      entries.push({
        id,
        input: publicInput,
        context,
        downloadId,
        attempts: 0,
        ready: startPump,
        nextAt: 0,
        createdAt: Date.now(),
      });
    });
    if (startPump) void this.pump();
    return id;
  }
  async enqueueConfirmed(input, context = {}) {
    const publicInput = structuredClone(input);
    delete publicInput.credentials;
    const id = await this.mutate((entries) => {
      const existing = entries.find(
        (entry) =>
          entry.downloadId === undefined &&
          entry.input.url === input.url &&
          entry.input.filename === input.filename &&
          entry.input.referer === input.referer &&
          JSON.stringify(entry.input.media) === JSON.stringify(input.media) &&
          entry.context?.tabId === context.tabId &&
          entry.context?.frameId === context.frameId,
      );
      if (existing) return existing.id;
      if (entries.length >= 500) throw new Error("Transfer queue is full");
      const id = crypto.randomUUID();
      entries.push({
        id,
        input: publicInput,
        context,
        attempts: 0,
        ready: false,
        nextAt: 0,
        createdAt: Date.now(),
      });
      return id;
    });
    const pending = this.#confirmations.get(id);
    if (pending) return pending.promise;
    let resolve, reject;
    const promise = new Promise((accept, decline) => {
      resolve = accept;
      reject = decline;
    });
    const timer = setTimeout(() => {
      this.#confirmations.delete(id);
      reject(
        new Error(
          "Desktop confirmation timed out; the transfer remains queued",
        ),
      );
    }, 35000);
    this.#confirmations.set(id, { promise, resolve, reject, timer });
    void this.activate(id).catch((error) => this.#confirm(id, null, error));
    return promise;
  }
  #confirm(id, task, error) {
    const pending = this.#confirmations.get(id);
    if (!pending) return;
    clearTimeout(pending.timer);
    this.#confirmations.delete(id);
    if (error) pending.reject(error);
    else pending.resolve({ id, task });
  }
  async enqueueBatch(inputs, context = {}) {
    if (!Array.isArray(inputs) || inputs.length < 1 || inputs.length > 500)
      throw new Error("Batch must contain 1 to 500 transfers");
    const candidates = new Map();
    for (const input of inputs) {
      if (!input || typeof input.url !== "string")
        throw new Error("Invalid batch input");
      const clean = structuredClone(input);
      delete clean.credentials;
      if (!candidates.has(clean.url)) candidates.set(clean.url, clean);
    }
    const ids = await this.mutate((entries) => {
      const existing = new Map(
        entries
          .filter((entry) => entry.command === "add")
          .map((entry) => [entry.input.url, entry.id]),
      );
      const additions = [...candidates.values()].filter(
        (input) => !existing.has(input.url),
      );
      if (entries.length + additions.length > 500)
        throw new Error("Transfer queue is full");
      const createdAt = Date.now();
      for (const input of additions) {
        const id = crypto.randomUUID();
        existing.set(input.url, id);
        entries.push({
          id,
          input,
          context: structuredClone(context),
          command: "add",
          attempts: 0,
          nextAt: 0,
          createdAt,
        });
      }
      return [...candidates.keys()].map((url) => existing.get(url));
    });
    void this.pump();
    return ids;
  }
  async activate(id) {
    await this.mutate((entries) => {
      const item = entries.find((entry) => entry.id === id);
      if (item) {
        item.ready = true;
        item.nextAt = 0;
      }
    });
    return this.pump();
  }
  pump() {
    if (this.#pumping) return this.#pumping;
    this.#pumping = this.#drain().finally(() => {
      this.#pumping = undefined;
    });
    return this.#pumping;
  }
  async #drain() {
    for (;;) {
      const entry = await this.mutate((entries) =>
        entries.find(
          (item) => item.ready !== false && item.nextAt <= Date.now(),
        ),
      );
      if (!entry) break;
      try {
        const prepared = entry.prepared
          ? entry.input
          : await this.prepare(entry);
        await this.mutate((entries) => {
          const item = entries.find((item) => item.id === entry.id);
          if (item) {
            item.input = prepared;
            item.prepared = true;
          }
        });
        entry.input = prepared;
        const task = await this.bridge.request(
          entry.command === "add" ? "add" : this.command,
          entry.input,
          entry.id,
        );
        // Finalize browser ownership before removing the durable entry. Replays are idempotent.
        if ((await this.onResult(entry, task, null)) === false) {
          await this.mutate((entries) => {
            const item = entries.find((item) => item.id === entry.id);
            if (item) item.nextAt = Date.now() + 1500;
          });
          continue;
        }
        await this.mutate((entries) => {
          const index = entries.findIndex((item) => item.id === entry.id);
          if (index >= 0) entries.splice(index, 1);
        });
        this.#confirm(entry.id, task, null);
      } catch (error) {
        const terminal =
          /invalid|reserved|already exists|already reserved|must be|forbidden|reused|unsupported protocol/i.test(
            error.message,
          );
        await this.mutate((entries) => {
          const item = entries.find((item) => item.id === entry.id);
          if (!item) return;
          if (terminal) entries.splice(entries.indexOf(item), 1);
          else {
            item.attempts++;
            item.nextAt =
              Date.now() +
              Math.min(300000, 2000 * 2 ** Math.min(item.attempts, 7));
          }
        });
        this.#confirm(entry.id, null, error);
        await this.onResult(entry, null, error);
        if (!terminal) break;
      }
    }
  }
}
