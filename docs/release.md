# Release and code signing

The release workflow builds Windows x64 on a GitHub-hosted runner. Tags must match the version in `package.json`. Manual runs produce artifacts by default; publication creates a draft GitHub release.

The current release is **2.2.0**. Its installer and portable runtime are validated on Windows x64. Keep both README files, the package version, extension manifests and `docs/releases/v2.2.0.md` aligned. Historical documents retain their original version and evidence dates.

## Clean build

The workflow provisions Rust 1.99.0, Node 24.18.0 and MSYS2. `bootstrap-media-toolchain.ps1` retrieves the pinned LLVM MinGW archive with a checked SHA-256, then compiles pinned FFmpeg, libxml2 and LAME sources. The Rust application uses the standard Windows MSVC toolchain. The LGPL media build is separate from the GPL-capable fixture encoder used only by tests.

For a local Windows build with MSYS2 available:

```powershell
npm ci
./scripts/bootstrap-media-toolchain.ps1 -PkgconfBin (Join-Path $env:MSYS2_ROOT 'ucrt64/bin') -Bash (Join-Path $env:MSYS2_ROOT 'usr/bin/bash.exe')
./scripts/build-release.ps1
./scripts/test-installer.ps1
node ./scripts/test-daily-use.mjs target/release
./scripts/export-release.ps1
./scripts/verify-release.ps1
./scripts/finalize-public-release.ps1
```

Set `MSYS2_ROOT` to the actual MSYS2 installation directory. An existing custom runtime can instead be supplied through `build-release.ps1 -MediaRuntime`. `-UseLocalToolchain` is an explicit opt-in for an already installed workspace Rust/LLVM toolchain.

Finalization reads `docs/releases/v<version>.md` for update notes and refuses a missing version-specific file. It creates the installer signature, `latest.json`, `SHA256SUMS.txt` and `release-info.json` in `public-release/` (or an explicit workspace output directory).

## Signed application updates

`TAURI_SIGNING_PRIVATE_KEY` is required by the release build and finalization. Locally, a configured key can be read from the user's `FluxSigning/updater.key` directory outside the workspace. CI uses GitHub's encrypted secret, with `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when needed. Never commit the private key or add it to source/runtime archives.

Update signing is independent of Authenticode. Flux checks signatures against its embedded public key and requires the trusted application version to match the offered release. The current Windows packages have signed update payloads but no Authenticode certificate. `release-info.json` records both states explicitly.

The main desktop checks `https://github.com/Hactor00/Flux/releases/latest/download/latest.json` shortly after startup; a manual check is available in Settings. Download and installation require a user action. Before installation, active tasks are checkpointed; a failed preparation is rolled back. A release must publish the matching installer, `.sig` and `latest.json` together. Do not replace assets of an already public version with different bytes.

For a manual publication, create a draft at the exact tested source commit, upload all packages and metadata, compare uploaded sizes and SHA-256 digests with the local artifacts, and publish it as the latest stable release only after verification. CI rebuilds preserve an existing release's assets. Verify public download URLs and the updater endpoint after publication.

The workflow captures the frontend/core test logs, records fresh native test results and binary hashes, runs Chromium HUD rendering checks, then exports the validation report with the source. `record-hud-validation.mjs` combines the Chromium and Firefox results. `record-release-validation.mjs` reads `.tools/frontend-tests.log`, `.tools/core-tests.log` and `.tools/production-audit.json`; it refuses missing counts, mismatched validation versions or HUD evidence for a different source hash.

## SignPath configuration

Install the SignPath GitHub connector and complete SignPath Foundation project review before enabling production signing. The workflow does not enroll a project or issue a certificate.

Repository secret:

- `SIGNPATH_API_TOKEN`: a submitter token restricted to the project and signing policy.

Repository variables:

- `SIGNPATH_ORGANIZATION_ID`
- `SIGNPATH_PROJECT_SLUG`
- `SIGNPATH_SIGNING_POLICY_SLUG`
- `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG`, when a non-default configuration is required
- `SIGNPATH_ENABLED=true`, to enable signing for release runs

Alternatively select the `sign` input in a manual run. A requested signed build fails when configuration is missing. It never silently labels an unsigned package as signed.

The unsigned package is uploaded as an immutable GitHub artifact. `SignPath/github-action-submit-signing-request@v2` submits its artifact ID and waits for completion. The artifact configuration must handle the NSIS container and portable ZIP, signing the owned application binaries inside each: `flux-desktop.exe`, `flux-cli.exe` and `flux-native-host.exe`. Configure nested NSIS signing in SignPath so installed executables are signed as well as the outer installer. Keep third-party files and licenses intact.

The finalization script requires valid Authenticode signatures on the returned installer and owned portable executables. It recomputes checksums after signing and records the signing state in `release-info.json`. Signing credentials are unavailable to pull-request workflows.

## Code signing policy

Only source-controlled release commits built on GitHub-hosted runners enter the signing workflow. SignPath approval roles and production certificate access belong to the project's designated maintainers. Use a review policy for production signing. Downloaded dependencies use pinned source versions and SHA-256 checks. Signing does not imply that every download source or third-party program is safe.

Reference: [SignPath GitHub integration](https://docs.signpath.io/trusted-build-systems/github).
