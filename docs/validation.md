# Current validation

The current release is **Flux 2.3.0**. See [2.3 validation](validation-2.3.md) and the [platform release record](validation/release-2.3.json). The [Windows package record](validation/release.json) contains the separately executed installer/browser checks.

## Flux 2.3

- Windows regression suite: 202 automated tests, comprising 124 Rust, 63 browser extension and 15 frontend tests.
- Watched-torrent folder settings, stable-file detection, persistent duplicate prevention and failure isolation.
- Linux x86_64 RPM build and Fedora installed-package validation, including native libraries, desktop startup, Native Messaging and watched-torrent queue admission.
- Real Chrome 155 and Firefox 120/157 checks, including foreground desktop approval without a browser file chooser, consent, HUD visibility, dragging and dismissal.
- Native Windows media/torrent transfers, separate windows, restart recovery, installer repair, removal and data retention.
- Formatting, strict TypeScript, production build, Clippy, dependency audit and Firefox manifest lint.
- Package identity and version-bound update signature verification.

See the detailed platform records for test conditions, final workflow identities and untested external scenarios. Evidence dates identify individual runs. Historical external network checks are not represented as fresh 2.3 checks.

## Historical records

| Version | Record |
| --- | --- |
| 2.2 | [Validation](validation-2.2.md) |
| 2.1 | [Validation](validation-2.1.md) |
| 2.0 | [Validation](validation-2.0.md) |
| 1.7 | [Validation](validation-1.7.md) · [HUD repair](validation-hud-1.7.md) |
| 1.6 | [Validation](validation-1.6.md) |
| 1.5 | [Validation](validation-1.5.md) |
| 1.4 and earlier | [Historical report](validation-1.4.md) |

Source and binary versions are checked by the release scripts. Current machine-readable files can be refreshed by a new release validation; use Git history to recover prior evidence snapshots.
