import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const helper = fileURLToPath(
  new URL("./native-test-state.ps1", import.meta.url),
);
const powershell = join(
  process.env.SystemRoot || "C:/Windows",
  "System32/WindowsPowerShell/v1.0/powershell.exe",
);
export function nativeRegistration(mode = "Read", args = []) {
  return execFileSync(
    powershell,
    [
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      helper,
      "-Mode",
      mode,
      ...args,
    ],
    { encoding: "utf8", windowsHide: true },
  ).trim();
}
export function preserveNativeRegistration() {
  if (process.platform !== "win32") return () => {};
  const previous = JSON.parse(nativeRegistration());
  const snapshot = join(
    mkdtempSync(join(tmpdir(), "flux-registration-")),
    "snapshot.json",
  );
  writeFileSync(snapshot, JSON.stringify(previous));
  return () => {
    nativeRegistration("Restore", ["-Snapshot", snapshot]);
    assert.deepEqual(
      JSON.parse(nativeRegistration()),
      previous,
      "Native registrations were not restored",
    );
  };
}
