import test from "node:test";
import assert from "node:assert/strict";
import { DurableOutbox } from "../bridge.js";
import { makeInput, isProviderUrl, isCollectionUrl } from "../shared.js";

function storage() {
  const data = { outbox: [] };
  globalThis.chrome = {
    storage: {
      local: {
        get: async (key) => structuredClone({ [key]: data[key] }),
        set: async (value) => Object.assign(data, structuredClone(value)),
      },
    },
  };
  return data;
}
async function until(condition) {
  const end = Date.now() + 3000;
  while (!condition()) {
    assert.ok(Date.now() < end);
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

test("atomic collection batches deduplicate URLs, retain UUIDs and replay into Add rather than approval windows", async () => {
  const data = storage();
  const offline = new DurableOutbox(
    {
      request: async () => {
        throw Error("Desktop offline");
      },
    },
    async () => {},
    async (entry) => entry.input,
    "propose",
  );
  const inputs = [
    makeInput("https://cdn.test/one.mp4"),
    makeInput("https://cdn.test/two.mp4"),
    makeInput("https://cdn.test/one.mp4"),
  ];
  const ids = await offline.enqueueBatch(inputs, { tabId: 4 });
  assert.equal(ids.length, 2);
  await until(() => data.outbox[0].attempts === 1);
  await offline.pump();
  const repeated = await offline.enqueueBatch(inputs, { tabId: 4 });
  assert.deepEqual(repeated, ids);
  assert.equal(data.outbox.length, 2);
  await offline.pump();
  const sent = [];
  const replay = new DurableOutbox(
    {
      request: async (command, input, id) => {
        sent.push({ command, url: input.url, id });
        return { id };
      },
    },
    async () => {},
    async (entry) => entry.input,
    "propose",
  );
  await replay.mutate((entries) => {
    for (const entry of entries) entry.nextAt = 0;
  });
  await replay.pump();
  assert.equal(data.outbox.length, 0);
  assert.deepEqual(
    sent.map((item) => item.id),
    ids,
  );
  assert.deepEqual(
    sent.map((item) => item.command),
    ["add", "add"],
  );
});

test("batch capacity failures do not partially persist any new tasks", async () => {
  const data = storage();
  const queue = new DurableOutbox({
    request: async () => {
      throw Error("Desktop offline");
    },
  });
  await queue.enqueueBatch(
    Array.from({ length: 500 }, (_, index) =>
      makeInput(`https://cdn.test/${index}.mp4`),
    ),
  );
  await assert.rejects(
    queue.enqueueBatch([makeInput("https://cdn.test/overflow.mp4")]),
    /queue is full/,
  );
  assert.equal(data.outbox.length, 500);
  assert.ok(
    !data.outbox.some((entry) => entry.input.url.endsWith("overflow.mp4")),
  );
});

test("collection and provider predicates require exact host boundaries", () => {
  assert.ok(isProviderUrl("https://player.vimeo.com/video/123"));
  assert.ok(isCollectionUrl("https://www.youtube.com/@channel/videos"));
  assert.ok(isCollectionUrl("https://www.youtube.com/watch?v=1&list=PL1"));
  assert.ok(isCollectionUrl("https://soundcloud.com/artist/sets/album"));
  assert.ok(!isProviderUrl("https://youtube.com.attacker.test/watch"));
  assert.ok(!isProviderUrl("https://user:password@youtube.com/watch"));
  assert.ok(!isCollectionUrl("https://youtube.com/watch?v=1"));
});

test("oversized batch metadata is rejected before any durable queue write", async () => {
  const data = storage();
  const queue = new DurableOutbox({
    request: async () => {
      throw Error("Desktop offline");
    },
  });
  await assert.rejects(
    queue.enqueueBatch(
      Array.from({ length: 500 }, (_, index) => ({
        ...makeInput(`https://cdn.test/${index}.mp4`),
        filename: "x".repeat(18000),
      })),
    ),
    /metadata exceeds/,
  );
  assert.equal(data.outbox.length, 0);
});
