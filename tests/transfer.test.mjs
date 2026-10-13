import test from "node:test";
import assert from "node:assert/strict";
import { transferView, formatDuration } from "../src/transfer.ts";
const task = {
  id: "film",
  status: "running",
  completed: 0,
  total: null,
  request: { media: { container: "mp4" } },
};
const media = {
  duration_ms: 3600000,
  processed_ms: 900000,
  segments_total: 600,
  segments_completed: 150,
  percent: 25,
  eta_seconds: 120,
  phase: "downloading",
};
test("unknown-size film receives nonzero duration progress and remaining time", () => {
  const view = transferView(task, {
    id: "film",
    completed: 240000000,
    total: 960000000,
    total_estimated: true,
    bytes_per_second: 4300000,
    media,
  });
  assert.equal(view.percent, 25);
  assert.equal(view.eta, 120);
  assert.equal(view.estimated, true);
  assert.equal(view.percentEstimated, true);
  assert.equal(formatDuration(view.eta), "00:02:00");
});
test("reopened details window uses saved media progress before its first live event", () => {
  const view = transferView({
    ...task,
    completed: 240000000,
    media_progress: media,
  });
  assert.equal(view.percent, 25);
  assert.equal(view.eta, 120);
});
test("completed task ignores stale transfer events", () => {
  const view = transferView(
    { ...task, status: "complete", completed: 500, total: 500 },
    { id: "film", completed: 250, total: null, media },
  );
  assert.equal(view.percent, 100);
  assert.equal(view.done, 500);
  assert.equal(view.eta, null);
});
test("pause retains progress but removes an obsolete countdown", () => {
  const view = transferView({
    ...task,
    status: "paused",
    media_progress: media,
  });
  assert.equal(view.percent, 25);
  assert.equal(view.eta, null);
});
test("live stream has no fictitious finish time or percent", () => {
  const view = transferView(task, {
    id: "film",
    completed: 3000,
    total: null,
    bytes_per_second: 1000,
    media: { ...media, phase: "live", percent: null, eta_seconds: null },
  });
  assert.equal(view.percent, null);
  assert.equal(view.eta, null);
  assert.equal(view.phase, "live");
});
test("unknown file size is indeterminate rather than stuck at zero percent", () => {
  assert.equal(
    transferView({ ...task, request: { media: null } }).percent,
    null,
  );
  assert.equal(formatDuration(3723.8), "01:02:03");
});
test("verified completion is required before media or torrent progress reaches 100 percent", () => {
  const mediaView = transferView({
    ...task,
    status: "verifying",
    media_progress: { ...media, percent: 100 },
  });
  assert.equal(mediaView.percent, 99.9);
  assert.equal(mediaView.phase, "processing");
  const torrentView = transferView({
    ...task,
    status: "verifying",
    request: { torrent: {} },
    completed: 100,
    total: 100,
  });
  assert.equal(torrentView.percent, 99.9);
  assert.equal(torrentView.phase, "verifying");
  assert.equal(torrentView.eta, null);
  assert.equal(transferView({ ...task, status: "complete" }).percent, 100);
});
