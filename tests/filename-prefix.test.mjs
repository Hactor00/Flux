import test from "node:test";
import assert from "node:assert/strict";
import {
  filenamePrefixHistory,
  filenamePrefixPreview,
  rememberFilenamePrefix,
  splitFilenamePrefix,
} from "../src/filename-prefix.ts";

function storage() {
  const data = new Map();
  return {
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
  };
}

test("prefixes preserve extensions and support nested folders on Windows and Linux", () => {
  assert.deepEqual(splitFilenamePrefix("Courses/Rust/lesson-", "video.mp4"), {
    prefix: "Courses/Rust/lesson-",
    folders: ["Courses", "Rust"],
    filename: "lesson-video.mp4",
  });
  assert.equal(
    filenamePrefixPreview("C:\\Downloads", "Courses/Rust/lesson-", "video.mp4"),
    "C:\\Downloads\\Courses\\Rust\\lesson-video.mp4",
  );
  assert.equal(
    filenamePrefixPreview("/home/user/Downloads", "Courses/", "video.mp4"),
    "/home/user/Downloads/Courses/video.mp4",
  );
  assert.equal(
    splitFilenamePrefix("Courses\\lesson-", "file.zip").prefix,
    "Courses/lesson-",
  );
});

test("prefixes reject traversal, absolute paths, reserved names, controls and oversized output names", () => {
  for (const prefix of [
    "../",
    "/root/",
    "C:/",
    "a//b/",
    "a/../",
    "CON/",
    "a./",
    "a /",
    "\0",
    "bad:",
    "a/../bad-",
    "x".repeat(200),
  ])
    assert.throws(
      () => splitFilenamePrefix(prefix, "file.zip"),
      undefined,
      prefix,
    );
  assert.throws(() => splitFilenamePrefix("a/", "../file.zip"));
  assert.throws(() => splitFilenamePrefix("é".repeat(100), "x"));
});

test("history persists ten unique most recent prefixes and promotes reused entries", () => {
  const saved = storage();
  for (let i = 0; i < 12; i++) rememberFilenamePrefix(`Series/${i}-`, saved);
  assert.deepEqual(
    filenamePrefixHistory(saved),
    Array.from({ length: 10 }, (_, index) => `Series/${11 - index}-`),
  );
  rememberFilenamePrefix("Series\\5-", saved);
  const history = filenamePrefixHistory(saved);
  assert.equal(history[0], "Series/5-");
  assert.equal(history.length, 10);
  assert.equal(new Set(history).size, 10);
  assert.deepEqual(rememberFilenamePrefix("", saved), history);
  assert.equal(rememberFilenamePrefix("x".repeat(199), saved)[0].length, 199);
});

test("malformed history and inaccessible storage do not break the download form", () => {
  assert.deepEqual(filenamePrefixHistory({ getItem: () => "not JSON" }), []);
  assert.deepEqual(
    filenamePrefixHistory({ getItem: () => '{"invalid":true}' }),
    [],
  );
  assert.deepEqual(
    filenamePrefixHistory({ getItem: () => '["../", 4, "", "ok-", "ok-"]' }),
    ["ok-"],
  );
  const blocked = {
    getItem: () => {
      throw new Error("Unavailable");
    },
    setItem: () => {
      throw new Error("Quota");
    },
  };
  assert.deepEqual(rememberFilenamePrefix("valid-", blocked), ["valid-"]);
});
