#!/usr/bin/env python3
"""Exercise the installed Linux package as an unprivileged desktop user."""
import ctypes
import hashlib
import http.server
import json
import os
from pathlib import Path
import sqlite3
import struct
import subprocess
import tempfile
import threading
import time
import uuid


def run(*arguments):
    return subprocess.run(arguments, check=True, capture_output=True, text=True, timeout=60).stdout


def bencode(value):
    if isinstance(value, bytes):
        return str(len(value)).encode() + b":" + value
    if isinstance(value, str):
        return bencode(value.encode())
    if isinstance(value, int):
        return b"i" + str(value).encode() + b"e"
    return b"d" + b"".join(bencode(key) + bencode(value[key]) for key in sorted(value)) + b"e"


class Fixture(http.server.BaseHTTPRequestHandler):
    payload = bytes(range(251)) * 4096

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Length", str(len(self.payload)))
        self.end_headers()
        self.wfile.write(self.payload)

    def do_HEAD(self):
        self.send_response(200)
        self.send_header("Content-Length", str(len(self.payload)))
        self.end_headers()

    def log_message(self, *arguments):
        pass


def native_list(origin):
    message = json.dumps({"version": 1, "request_id": str(uuid.uuid4()), "command": "list"}).encode()
    response = subprocess.run(["/usr/bin/flux-native-host", origin], input=struct.pack("<I", len(message)) + message, capture_output=True, timeout=25, check=True).stdout
    length, = struct.unpack("<I", response[:4])
    result = json.loads(response[4:4 + length])
    assert result["ok"], result.get("error")
    return result["result"]


def main():
    assert os.getuid() != 0, "Run this test as a normal desktop user"
    version = run("flux-cli", "--version").strip()
    assert version.endswith("2.3.0"), version
    assert ctypes.CDLL("/usr/lib/flux/libflux_torrent.so").flux_torrent_abi() == 1
    run("/usr/lib/flux/yt-dlp", "--version")
    run("ffmpeg", "-version")
    run("ffprobe", "-version")
    run("node", "--version")
    root = Path(tempfile.mkdtemp(prefix="flux-package-"))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    run("flux-cli", "--data", str(root / "http-data"), "--output", str(root / "http-downloads"), "download", f"http://127.0.0.1:{server.server_port}/fixture.bin", "fixture.bin", "--sha256", hashlib.sha256(Fixture.payload).hexdigest())
    assert (root / "http-downloads/fixture.bin").read_bytes() == Fixture.payload
    server.shutdown()
    watch = root / "watch"
    watch.mkdir()
    (watch / "sample.torrent").write_bytes(bencode({"info": {"length": 7, "name": "sample.bin", "piece length": 16384, "pieces": hashlib.sha1(b"fixture").digest()}}))
    data = Path.home() / ".local/share/Flux"
    downloads = Path.home() / "Downloads/Flux"
    downloads.mkdir(parents=True, exist_ok=True)
    configuration = root / "settings.json"
    configuration.write_text(json.dumps({"locale": "en-US", "notifications": False, "queue_enabled": False, "torrent_watch": {"enabled": True, "folders": [str(watch)], "mode": "automatic"}}))
    run("flux-cli", "--data", str(data), "--output", str(downloads), "settings", "--input", str(configuration))
    run("flux-desktop", "--register-browser")
    manifest = json.loads((data / "native-host.json").read_text())
    assert manifest["path"] == "/usr/bin/flux-native-host"
    assert (Path.home() / ".mozilla/native-messaging-hosts/io.flux.download.json").is_file()
    log = (root / "desktop.log").open("w")
    desktop = subprocess.Popen(["flux-desktop"], stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            if desktop.poll() is not None:
                raise RuntimeError((root / "desktop.log").read_text())
            if (data / "broker.json").is_file():
                break
            time.sleep(0.2)
        else:
            raise AssertionError("Desktop broker did not start")
        run("xdotool", "search", "--sync", "--onlyvisible", "--name", "^Flux$")
        tasks = []
        while time.monotonic() < deadline:
            tasks = native_list(manifest["allowed_origins"][0])
            if tasks:
                break
            time.sleep(0.3)
        assert len(tasks) == 1 and tasks[0]["status"] == "queued", tasks
        time.sleep(4)
        assert len(native_list(manifest["allowed_origins"][0])) == 1
        run("flux-desktop", "--shutdown")
        assert desktop.wait(timeout=35) == 0
        with sqlite3.connect(data / "tasks.sqlite") as connection:
            assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        print(json.dumps({"version": version, "http_sha256": True, "native_bep52_runtime": True, "desktop_window": True, "browser_manifests": True, "native_messaging": True, "watched_torrent": True, "duplicate_prevention": True, "sqlite_integrity": True}))
    finally:
        if desktop.poll() is None:
            desktop.terminate()
            desktop.wait(timeout=15)
        log.close()


if __name__ == "__main__":
    main()
