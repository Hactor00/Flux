#!/usr/bin/env python3
"""Assemble tested platform packages without rebuilding or replacing signed binaries."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.parse
import zipfile


def run(*arguments):
    return subprocess.check_output(arguments, text=True).strip()


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def api(path, method="GET", body=None):
    arguments = [
        "gh", "api", f"repos/{os.environ['GITHUB_REPOSITORY']}/{path}",
        "--method", method, "--header", "X-GitHub-Api-Version: 2022-11-28",
        "--header", "Accept: application/vnd.github+json", "--header", "Cache-Control: no-cache",
    ]
    if body is not None:
        arguments.extend(["--input", "-"])
    response = subprocess.check_output(arguments, input=json.dumps(body) if body is not None else None, text=True)
    return json.loads(response)


def identities(releases):
    return [(item["id"], [(asset["id"], asset["name"], asset["size"], asset.get("digest")) for asset in item["assets"]]) for item in releases if not item["draft"]]


def refresh_published(stage, names, existing, version, release_info, previous):
    """Replace only an explicitly requested version, retaining verified rollback files."""
    tag = f"v{version}"
    if os.environ.get("REFRESH_VERSION") != version or existing.get("immutable"):
        raise RuntimeError("Published release refresh was not explicitly authorized or the release is immutable")
    if {asset["name"] for asset in existing["assets"]} != set(names):
        raise RuntimeError("Existing release contains an unexpected asset set")
    release_id = existing["id"]
    backup = stage / "previous-published"
    backup.mkdir(exist_ok=True)
    old_ref = api(f"git/ref/tags/{tag}")
    (backup / "release-metadata.json").write_text(json.dumps(existing, indent=2) + "\n")
    (backup / "tag-reference.json").write_text(json.dumps(old_ref, indent=2) + "\n")
    subprocess.run(["gh", "release", "download", tag, "--dir", str(backup)], check=True)
    for asset in existing["assets"]:
        path = backup / asset["name"]
        if path.stat().st_size != asset["size"] or asset.get("digest") != "sha256:" + digest(path):
            raise RuntimeError("Previously published package backup could not be verified")
    others = [item for item in previous if item[0] != release_id]
    # Keep the old signed feed until all replacement installer/package uploads are verified.
    ordered = [name for name in names if name != "latest.json"] + ["latest.json"]
    changed = False
    try:
        for name in ordered:
            current = api(f"releases/{release_id}")
            if current["draft"] or current["tag_name"] != tag:
                raise RuntimeError("Published release state changed during refresh")
            asset = next(item for item in current["assets"] if item["name"] == name)
            if asset.get("digest") == "sha256:" + digest(stage / name):
                continue
            changed = True
            subprocess.run(["gh", "release", "upload", tag, str(stage / name), "--clobber"], check=True)
            uploaded = next(item for item in api(f"releases/{release_id}")["assets"] if item["name"] == name)
            if uploaded.get("digest") != "sha256:" + digest(stage / name) or uploaded["size"] != (stage / name).stat().st_size:
                raise RuntimeError(f"Refreshed package identity mismatch: {name}")
        api(f"git/refs/tags/{tag}", "PATCH", {"sha": release_info["source_commit"], "force": True})
        api(f"releases/{release_id}", "PATCH", {
            "target_commitish": release_info["source_commit"],
            "body": Path(f"docs/releases/{tag}.md").read_text(),
        })
        if others != [item for item in identities(api("releases?per_page=100")) if item[0] != release_id]:
            raise RuntimeError("Another published release changed during refresh")
        final = api(f"releases/{release_id}")
        if {asset["name"] for asset in final["assets"]} != set(names) or final["draft"]:
            raise RuntimeError("Refreshed release asset set is incomplete")
        print(f"Verified refreshed {tag} packages; previous files retained as a rollback artifact.")
    except Exception:
        if changed:
            for name in ordered:
                subprocess.run(["gh", "release", "upload", tag, str(backup / name), "--clobber"], check=True)
            api(f"git/refs/tags/{tag}", "PATCH", {"sha": old_ref["object"]["sha"], "force": True})
            api(f"releases/{release_id}", "PATCH", {"target_commitish": existing["target_commitish"], "body": existing["body"]})
        raise


def main():
    stage = Path(sys.argv[1]).resolve()
    version = read_json("package.json")["version"]
    tag = f"v{version}"
    windows = read_json(stage / "windows-validation" / "release.json")
    linux = read_json(stage / "linux-validation.json")
    if windows["version"] != version or linux["version"] != version or not linux["passed"]:
        raise RuntimeError("Platform validation version mismatch")
    for platform, variable in (("windows", "WINDOWS_RUN"), ("linux", "LINUX_RUN")):
        run_id = os.environ[variable]
        workflow = api(f"actions/runs/{run_id}")
        if workflow["conclusion"] != "success":
            raise RuntimeError(f"{platform} workflow did not pass")
        expected_workflow = (
            ".github/workflows/release.yml" if platform == "windows"
            else ".github/workflows/linux-release.yml"
        )
        if (
            workflow["path"] != expected_workflow
            or workflow["head_repository"]["full_name"] != os.environ["GITHUB_REPOSITORY"]
        ):
            raise RuntimeError(f"Unexpected {platform} build provenance")
        if platform == "linux" and (str(linux["run_id"]) != run_id or linux["source_commit"] != workflow["head_sha"]):
            raise RuntimeError("Linux validation record does not match its workflow run")
        subprocess.run(["git", "fetch", "--no-tags", "--depth=1", "origin", workflow["head_sha"]], check=True)
        runtime_paths = [
            "crates", "src", "src-tauri", "extension", "extension-firefox", "native", "native-host",
            "vendor", ".cargo", "Cargo.toml", "Cargo.lock", "package.json", "package-lock.json",
            "scripts/build-torrent-runtime.ps1", "scripts/build-torrent-runtime.sh",
            "scripts/bootstrap-torrent-runtime.sh", "scripts/prepare-torrent-runtime.py",
            "scripts/build-linux-release.sh", "scripts/build-extensions.mjs",
            "scripts/build-media-runtime.sh", "scripts/prepare-release.ps1",
            "scripts/build-release.ps1", "scripts/prepare-extractor.ps1",
            "scripts/portable-launcher.rs",
        ]
        if platform == "linux":
            runtime_paths = [path for path in runtime_paths if path not in (
                "scripts/build-media-runtime.sh", "scripts/prepare-release.ps1",
                "scripts/build-release.ps1", "scripts/prepare-extractor.ps1",
                "scripts/portable-launcher.rs",
            )]
        subprocess.run(["git", "diff", "--quiet", workflow["head_sha"], "HEAD", "--", *runtime_paths], check=True)
    original_sums = {}
    for line in (stage / "SHA256SUMS.txt").read_text(encoding="utf-8-sig").splitlines():
        checksum, name = line.split("  ", 1)
        if Path(name).name != name or digest(stage / name) != checksum:
            raise RuntimeError("Original Windows artifact checksum mismatch")
        original_sums[name] = checksum
    for suffix in ("windows-x64.zip", "portableapps.zip", "media-source.zip", "chrome.zip", "firefox.zip", "firefox-120.zip", "source.zip"):
        if f"Flux-{version}-{suffix}" not in original_sums:
            raise RuntimeError(f"Original checksums omit {suffix}")
    installer = stage / f"Flux_{version}_x64-setup.exe"
    if windows["version"] != version or digest(installer) != windows["installer"]["sha256"]:
        raise RuntimeError("Installer does not match its installation test")
    portable = stage / f"Flux-{version}-windows-x64.zip"
    prefix = f"Flux-{version}-windows-x64/"
    with zipfile.ZipFile(portable) as archive:
        for name, expected in windows["binary_sha256"].items():
            with archive.open(prefix + name) as stream:
                if hashlib.file_digest(stream, "sha256").hexdigest() != expected:
                    raise RuntimeError(f"Portable binary identity mismatch: {name}")
    expected_rpm = linux["package_sha256"]
    if digest(stage / f"Flux-{version}-linux-x86_64.rpm") != expected_rpm:
        raise RuntimeError("RPM does not match installed-package validation")
    original_portable = portable.with_suffix(".original.zip")
    portable.rename(original_portable)
    with zipfile.ZipFile(original_portable) as source, zipfile.ZipFile(portable, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as output:
        for entry in source.infolist():
            relative = entry.filename.removeprefix(prefix)
            if relative in ("README.md", "README.tr.md") or relative.startswith("docs/"):
                continue
            with source.open(entry) as incoming, output.open(entry, "w") as destination:
                shutil.copyfileobj(incoming, destination)
        paths = [Path("README.md"), Path("README.tr.md")]
        for path in paths:
            entry = zipfile.ZipInfo(prefix + path.as_posix(), (1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            output.writestr(entry, path.read_bytes())
    original_portable.unlink()
    source_archive = stage / f"Flux-{version}-source.zip"
    subprocess.run(["git", "archive", "--format=zip", f"--output={source_archive}", "HEAD"], check=True)
    validation_report = stage / "windows-validation" / "release.json"
    validated_archive = source_archive.with_suffix(".validated.zip")
    report_included = False
    with zipfile.ZipFile(source_archive) as original, zipfile.ZipFile(validated_archive, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as output:
        for entry in original.infolist():
            if entry.filename == "docs/validation/release.json":
                contents = validation_report.read_bytes()
                report_included = True
            else:
                contents = original.read(entry)
            output.writestr(entry, contents)
    if not report_included:
        validated_archive.unlink(missing_ok=True)
        raise RuntimeError("Source archive omits the version-specific validation report")
    validated_archive.replace(source_archive)
    manifest = read_json(stage / "latest.json")
    if manifest["version"] != version or not manifest["platforms"]["windows-x86_64"]["signature"]:
        raise RuntimeError("Signed updater manifest is invalid")
    update = manifest["platforms"]["windows-x86_64"]
    if update["url"] != f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/releases/download/{tag}/{installer.name}" or update["signature"] != (stage / (installer.name + ".sig")).read_text().strip():
        raise RuntimeError("Updater signature or download target changed")
    release_info = read_json(stage / "release-info.json")
    release_info.pop("platform", None)
    release_info.update({"source_commit": run("git", "rev-parse", "HEAD"), "platforms": ["Windows x64", "Linux x86_64 RPM"], "windows_run": os.environ["WINDOWS_RUN"], "linux_run": os.environ["LINUX_RUN"]})
    release_info["linux"] = {"distribution_tested": "Fedora 43", "architecture": "x86_64", "rpm_signed": False, "package_sha256": expected_rpm}
    (stage / "release-info.json").write_text(json.dumps(release_info, indent=2) + "\n")
    names = [f"Flux_{version}_x64-setup.exe", f"Flux-{version}-windows-x64.zip", f"Flux-{version}-portableapps.zip", f"Flux-{version}-media-source.zip", f"Flux-{version}-linux-x86_64.rpm", f"Flux-{version}-chrome.zip", f"Flux-{version}-firefox.zip", f"Flux-{version}-firefox-120.zip", f"Flux-{version}-source.zip", f"Flux_{version}_x64-setup.exe.sig", "latest.json", "release-info.json"]
    checksums = {name: digest(stage / name) for name in names}
    (stage / "SHA256SUMS.txt").write_text("".join(f"{checksums[name]}  {name}\n" for name in sorted(names)))
    names.append("SHA256SUMS.txt")
    previous = identities(api("releases?per_page=100"))
    releases = api("releases?per_page=100")
    existing = next((item for item in releases if item["tag_name"] == tag), None)
    if existing and not existing["draft"]:
        refresh_published(stage, names, existing, version, release_info, previous)
        return
    if not existing:
        existing = api("releases", "POST", {
            "tag_name": tag, "draft": True, "target_commitish": release_info["source_commit"],
            "name": f"Flux {version}", "body": Path(f"docs/releases/{tag}.md").read_text(),
        })
    release_id = existing["id"]
    for name in names:
        release = api(f"releases/{release_id}")
        target = api("commits/" + urllib.parse.quote(release["target_commitish"], safe=""))
        if not release["draft"] or target["sha"] != release_info["source_commit"]:
            raise RuntimeError("Draft release target changed")
        remote = next((asset for asset in release["assets"] if asset["name"] == name), None)
        if remote is None:
            subprocess.run(["gh", "release", "upload", tag, str(stage / name)], check=True)
        published = api(f"releases/{release_id}")
        asset = next(item for item in published["assets"] if item["name"] == name)
        if asset["size"] != (stage / name).stat().st_size or asset.get("digest") != "sha256:" + digest(stage / name):
            raise RuntimeError(f"Uploaded package identity mismatch: {name}")
    if previous != identities(api("releases?per_page=100")):
        raise RuntimeError("Previously published assets changed")
    if len(api(f"releases/{release_id}")["assets"]) != len(names):
        raise RuntimeError("Draft contains unexpected assets")
    published_release = api(f"releases/{release_id}", "PATCH", {"draft": False})
    if published_release["draft"] or published_release["tag_name"] != tag:
        raise RuntimeError("Release publication could not be verified")
    print(f"Verified and published all {len(names)} assets for {tag}.")


if __name__ == "__main__":
    main()
