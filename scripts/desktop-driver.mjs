import { once } from "node:events";
export class DesktopDriver {
  constructor(port) {
    this.base = `http://127.0.0.1:${port}`;
    this.sockets = [];
  }
  async absent(view) {
    const deadline = Date.now() + 5000;
    do {
      const targets = await (
        await fetch(this.base + "/json/list", {
          signal: AbortSignal.timeout(2000),
        })
      ).json();
      if (
        !targets.some((target) => {
          try {
            return new URL(target.url).searchParams.get("view") === view;
          } catch {
            return false;
          }
        })
      )
        return;
      await new Promise((resolve) => setTimeout(resolve, 100));
    } while (Date.now() < deadline);
    throw Error(`Native window ${view} did not close`);
  }
  async target(view, timeout = 60000) {
    const deadline = Date.now() + timeout;
    let observed = [];
    let discoveryError;
    do {
      try {
        const list = await (
          await fetch(this.base + "/json/list", {
            signal: AbortSignal.timeout(2000),
          })
        ).json();
        observed = list.map((item) => {
          try {
            const url = new URL(item.url);
            return `${url.protocol}//${url.host}${url.pathname}?view=${url.searchParams.get("view") || "main"}`;
          } catch {
            return "unavailable URL";
          }
        });
        const target = list.find((item) => {
          try {
            const url = new URL(item.url);
            return (
              url.hostname === "tauri.localhost" &&
              (url.searchParams.get("view") || "main") === view
            );
          } catch {
            return false;
          }
        });
        if (target) return target;
      } catch (error) {
        discoveryError = error.message;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    } while (Date.now() < deadline);
    throw Error(
      `Native window ${view} did not become available; observed targets: ${JSON.stringify(observed)}; discovery error: ${discoveryError || "none"}`,
    );
  }
  async connect(view) {
    const target = await this.target(view);
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await once(socket, "open");
    this.sockets.push(socket);
    let serial = 0;
    const pending = new Map();
    socket.addEventListener("message", (event) => {
      const value = JSON.parse(event.data);
      if (value.id) pending.get(value.id)?.(value);
    });
    const call = (method, params = {}) =>
      new Promise((resolve, reject) => {
        const id = ++serial;
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(Error(`Native ${method} timed out`));
        }, 15000);
        pending.set(id, (value) => {
          clearTimeout(timer);
          pending.delete(id);
          value.error
            ? reject(Error(value.error.message))
            : resolve(value.result);
        });
        socket.send(JSON.stringify({ id, method, params }));
      });
    const evaluate = async (expression) => {
      const value = await call("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
      }).catch((error) => {
        throw new Error(
          `Native evaluation failed: ${expression.slice(0, 160)}: ${error.message}`,
        );
      });
      if (value.exceptionDetails) {
        const exception = value.exceptionDetails.exception;
        throw Error(
          typeof exception?.value === "string"
            ? exception.value
            : exception?.description || value.exceptionDetails.text,
        );
      }
      return value.result?.value;
    };
    const ready = Date.now() + 30000;
    while (
      !(await evaluate(
        "!!(document.querySelector('#root')?.children.length || document.querySelector('[role=dialog],.surface-loading'))",
      ))
    ) {
      if (Date.now() > ready) throw Error("Native renderer did not load");
      await new Promise((r) => setTimeout(r, 100));
    }
    return { call, evaluate, id: target.id };
  }
  close() {
    for (const socket of this.sockets) socket.close();
  }
}
