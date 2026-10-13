import test from "node:test";
import assert from "node:assert/strict";
import {
  matchResource,
  pageSources,
  renewSource,
  resourceIdentity,
} from "../healing.js";

test("signature healing preserves content identity and rejects different paths/origins", () => {
  const old = "https://cdn.test/film.mp4?id=7&X-Amz-Signature=old&X-Amz-Date=1";
  assert.equal(
    matchResource(old, [
      "https://cdn.test/other.mp4?id=7",
      "https://other.test/film.mp4?id=7",
      "https://cdn.test/film.mp4?id=8",
    ]),
    undefined,
  );
  assert.equal(
    matchResource(old, ["https://cdn.test/film.mp4?X-Amz-Signature=new&id=7"]),
    "https://cdn.test/film.mp4?X-Amz-Signature=new&id=7",
  );
  assert.throws(() => resourceIdentity("file:///etc/passwd"));
});
test("silent refresh fetches the original page without creating, focusing or reloading tabs", async () => {
  const calls = [];
  const old = "https://cdn.test/film.mp4?token=old&id=7";
  const url = await renewSource(
    { ticket: "ticket", page_url: "https://site.test/watch", url: old },
    {
      tabs: { query: async () => [{ id: 8, url: "https://site.test/watch" }] },
      storage: { get: async () => ({ "media:8": [{ url: old, seenAt: 1 }] }) },
      fetchPage: async (page, options) => {
        assert.equal(page, "https://site.test/watch");
        assert.equal(options.credentials, "include");
        return new Response(
          '<script>{"url":"https:\\/\\/cdn.test\\/film.mp4?id=7\\u0026token=new"}</script>',
        );
      },
      credentialsFor: async () => [
        { origin: "https://cdn.test", cookie: "session=new" },
      ],
      bridge: {
        request: async (command, payload) => {
          calls.push([command, payload]);
          return command === "protect" ? "sealed" : null;
        },
      },
    },
  );
  assert.equal(url, "https://cdn.test/film.mp4?id=7&token=new");
  assert.equal(calls[1][0], "refresh_submit");
  assert.equal(calls[1][1].protected_credentials, "sealed");
  assert.ok(!JSON.stringify(calls[1]).includes("session=new"));
});
test("closed source pages and oversized refresh responses fail without substituting a resource", async () => {
  assert.throws(() =>
    pageSources("x".repeat(4 * 1024 * 1024 + 1), "https://site.test"),
  );
  await assert.rejects(
    renewSource(
      { page_url: "https://site.test/watch", url: "https://cdn.test/file" },
      { tabs: { query: async () => [] } },
    ),
    /no longer open/,
  );
});
