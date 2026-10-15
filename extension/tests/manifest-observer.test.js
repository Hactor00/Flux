import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";

const script = await readFile(
  new URL("../media-observer.js", import.meta.url),
  "utf8",
);
const manifest = "#EXTM3U\n#EXTINF:2,\nsegment.jpg\n#EXT-X-ENDLIST\n";

function fixture() {
  const dom = new JSDOM("<!doctype html>", {
    url: "https://player.test/watch",
    runScripts: "outside-only",
  });
  dom.window.Response = class extends Response {};
  dom.window.TextDecoder = TextDecoder;
  const messages = [];
  dom.window.postMessage = (message) => messages.push(message);
  return { dom, messages };
}

function response(
  dom,
  text,
  url = "https://cdn.test/opaque-token",
  mime = "image/jpeg",
) {
  const value = new dom.window.Response(text, {
    headers: { "content-type": mime },
  });
  Object.defineProperty(value, "url", { value: url });
  return value;
}

test("page fetch, CSP probe rejection and promise identity are unchanged", async () => {
  const { dom, messages } = fixture();
  const failure = new TypeError(
    "Blocked by the document Content Security Policy",
  );
  let requested;
  const pending = Promise.reject(failure);
  pending.catch(() => {});
  const fetch = (url) => {
    requested = url;
    return pending;
  };
  dom.window.fetch = fetch;
  dom.window.eval(script);
  assert.equal(dom.window.fetch, fetch);
  assert.equal(dom.window.fetch("data:,"), pending);
  await assert.rejects(pending, (error) => error === failure);
  assert.equal(requested, "data:,");
  assert.equal(messages.length, 0);
  dom.window.close();
});

test("consumed extensionless manifests are observed without cloning or reading an extra stream", async () => {
  const { dom, messages } = fixture();
  const value = response(dom, manifest);
  value.clone = () => {
    throw new Error("The observer must not clone page responses");
  };
  dom.window.eval(script);
  assert.equal(await value.text(), manifest);
  assert.equal(value.bodyUsed, true);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].url, value.url);
  assert.equal(messages[0].text, manifest);
  await assert.rejects(value.text(), TypeError);
  dom.window.close();
});

test("arrayBuffer consumers retain their bytes and reveal DASH manifests with misleading MIME types", async () => {
  const { dom, messages } = fixture();
  const text = '<?xml version="1.0"?><MPD><Period/></MPD>';
  const value = response(
    dom,
    text,
    "https://cdn.test/dynamic",
    "application/octet-stream",
  );
  dom.window.eval(script);
  const bytes = await value.arrayBuffer();
  assert.equal(new TextDecoder().decode(bytes), text);
  assert.equal(messages[0].text, text);
  dom.window.close();
});

test("oversized, non-network and non-manifest bodies are not published", async () => {
  const { dom, messages } = fixture();
  dom.window.eval(script);
  for (const [text, url] of [
    [manifest + "x".repeat(128 * 1024), "https://cdn.test/master.m3u8"],
    [manifest, "data:,"],
    ["<html>Page</html>", "https://player.test/"],
  ]) {
    assert.equal(await response(dom, text, url).text(), text);
  }
  await response(dom, new Uint8Array(256 * 1024)).arrayBuffer();
  assert.equal(messages.length, 0);
  dom.window.close();
});

test("unconsumed responses do not create background readers or timers", () => {
  const { dom, messages } = fixture();
  const value = response(dom, manifest);
  const fetch = () => Promise.resolve(value);
  dom.window.fetch = fetch;
  dom.window.setTimeout = () => {
    throw new Error("No inspection timers are needed");
  };
  dom.window.eval(script);
  assert.equal(dom.window.fetch, fetch);
  assert.equal(value.bodyUsed, false);
  assert.equal(messages.length, 0);
  dom.window.close();
});
