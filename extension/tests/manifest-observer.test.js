import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";

const script = await readFile(
  new URL("../media-observer.js", import.meta.url),
  "utf8",
);

test("stalled inspection releases its slots without aborting the page request", async () => {
  const dom = new JSDOM("<!doctype html>", {
    url: "https://player.test/watch",
    runScripts: "outside-only",
  });
  const schedule = dom.window.setTimeout.bind(dom.window);
  dom.window.setTimeout = (callback, delay) =>
    schedule(callback, delay === 3000 ? 20 : delay);
  let cancelled = 0;
  const response = {
    url: "https://cdn.test/master.m3u8",
    body: {},
    headers: new Headers({ "content-type": "application/vnd.apple.mpegurl" }),
    clone: () => ({
      body: {
        getReader: () => ({
          read: () => new Promise(() => {}),
          cancel: () => {
            cancelled++;
            return Promise.resolve();
          },
        }),
      },
    }),
  };
  dom.window.TextDecoder = TextDecoder;
  dom.window.fetch = () => Promise.resolve(response);
  dom.window.eval(script);
  assert.equal(await dom.window.fetch(response.url), response);
  assert.equal(await dom.window.fetch(response.url), response);
  await new Promise((resolve) => setTimeout(resolve, 60));
  assert.equal(cancelled, 2);
  await dom.window.fetch(response.url);
  await new Promise((resolve) => setTimeout(resolve, 60));
  assert.equal(cancelled, 3);
  dom.window.close();
});

test("manifest observation preserves the page response and recognizes misleading MIME types", async () => {
  const dom = new JSDOM("<!doctype html>", {
    url: "https://player.test/watch",
    runScripts: "outside-only",
  });
  const text = "#EXTM3U\n#EXTINF:2,\nsegment.jpg\n#EXT-X-ENDLIST\n";
  const response = new Response(text, {
    headers: { "content-type": "image/jpeg" },
  });
  Object.defineProperty(response, "url", {
    value: "https://cdn.test/opaque-token",
  });
  dom.window.TextDecoder = TextDecoder;
  dom.window.fetch = () => Promise.resolve(response);
  const messages = [];
  dom.window.addEventListener("message", (event) => messages.push(event.data));
  dom.window.eval(script);
  const returned = await dom.window.fetch("https://cdn.test/opaque-token");
  assert.equal(returned, response);
  assert.equal(await returned.text(), text);
  for (
    let attempts = 0;
    attempts < 30 && !messages.some((message) => message.kind === "manifest");
    attempts++
  )
    await new Promise((resolve) => setTimeout(resolve, 10));
  const hint = messages.find((message) => message.kind === "manifest");
  assert.equal(hint.url, "https://cdn.test/opaque-token");
  assert.equal(hint.text, text);
  dom.window.close();
});

test("oversized manifest responses are left to the page without copying them", async () => {
  const dom = new JSDOM("<!doctype html>", {
    url: "https://player.test/watch",
    runScripts: "outside-only",
  });
  const response = new Response("#EXTM3U", {
    headers: {
      "content-type": "application/vnd.apple.mpegurl",
      "content-length": "200000",
    },
  });
  Object.defineProperty(response, "url", {
    value: "https://cdn.test/master.m3u8",
  });
  dom.window.TextDecoder = TextDecoder;
  dom.window.fetch = () => Promise.resolve(response);
  const messages = [];
  dom.window.addEventListener("message", (event) => messages.push(event.data));
  dom.window.eval(script);
  assert.equal(await (await dom.window.fetch(response.url)).text(), "#EXTM3U");
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.equal(messages.length, 0);
  dom.window.close();
});
