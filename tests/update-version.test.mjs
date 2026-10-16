import test from "node:test";
import assert from "node:assert/strict";
import { compareVersions } from "../src/update-version.js";

test("the updater accepts only versions newer than the installed release", () => {
  assert.equal(compareVersions("2.4.1", "2.4.0"), 1);
  assert.equal(compareVersions("2.4.0", "2.4.0"), 0);
  assert.equal(compareVersions("2.3.9", "2.4.0"), -1);
  assert.equal(compareVersions("2.4", "2.4.0"), 0);
  assert.equal(compareVersions("latest", "2.4.0"), null);
  assert.equal(compareVersions("2.4.2junk", "2.4.0"), null);
  assert.equal(compareVersions(null, "2.4.0"), null);
});
