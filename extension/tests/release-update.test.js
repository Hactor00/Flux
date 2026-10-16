import test from "node:test";
import assert from "node:assert/strict";
import { ReleaseChecker, compareVersions } from "../release-update.js";

test("update notices require a strictly newer numeric version", () => {
  assert.equal(compareVersions("2.4.2", "2.4.1"), 1);
  assert.equal(compareVersions("2.4.2", "2.4.2"), 0);
  assert.equal(compareVersions("2.4.2", "2.5.0"), -1);
  for (const bad of [null, "2.4.3script", "v2.4.3", "2..4", "999999.0"])
    assert.equal(compareVersions(bad, "2.4.2"), null);
});

test("release checks cache, coalesce, refresh and retain valid notices offline", async () => {
  let time = 1_000_000;
  const data = {};
  const storage = {
    async get() {
      return data;
    },
    async set(value) {
      Object.assign(data, value);
    },
  };
  let calls = 0;
  const checker = new ReleaseChecker(
    storage,
    async (url, options) => {
      calls++;
      assert.equal(
        url,
        "https://github.com/Hactor00/Flux/releases/latest/download/release-info.json",
      );
      assert.equal(options.credentials, "omit");
      return new Response(
        JSON.stringify({ version: "2.4.3", url: "https://untrusted.invalid" }),
      );
    },
    () => time,
  );
  assert.deepEqual(await Promise.all([checker.check(), checker.check()]), [
    "2.4.3",
    "2.4.3",
  ]);
  assert.equal(calls, 1);
  assert.equal(await checker.check(), "2.4.3");
  assert.equal(calls, 1);
  time += 6 * 60 * 60 * 1000;
  assert.equal(await checker.check(), "2.4.3");
  assert.equal(calls, 2);
  time += 6 * 60 * 60 * 1000;
  const offline = new ReleaseChecker(
    storage,
    async () => {
      throw Error("offline");
    },
    () => time,
  );
  assert.equal(await offline.check(), "2.4.3");
  const invalid = new ReleaseChecker(
    {
      async get() {
        return {};
      },
      async set() {},
    },
    async () => new Response('{"version":"99.0<script>"}'),
  );
  assert.equal(await invalid.check(), null);
});
