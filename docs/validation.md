# Current validation

The current release is **Flux 2.2.0**. See [2.2 validation](validation-2.2.md) and the [machine-readable release record](validation/release.json).

## Flux 2.2

- 188 automated tests: 115 Rust, 59 browser extension and 14 frontend.
- Real Chrome 155 and Firefox 120/157 checks, including foreground desktop approval without a browser file chooser, consent, HUD visibility, dragging and dismissal.
- Native Windows media/torrent transfers, separate windows, restart recovery, installer repair, removal and data retention.
- Formatting, strict TypeScript, production build, Clippy, dependency audit and Firefox manifest lint.
- Package identity and version-bound update signature verification.

See the detailed record for test conditions and untested external/platform scenarios. Evidence dates identify individual runs; historical network tests are not represented as fresh 2.2 checks. Functional checks passed while a nonfatal upstream WebView2 cleanup warning was recorded.

## Historical records

| Version | Record |
| --- | --- |
| 2.1 | [Validation](validation-2.1.md) |
| 2.0 | [Validation](validation-2.0.md) |
| 1.7 | [Validation](validation-1.7.md) · [HUD repair](validation-hud-1.7.md) |
| 1.6 | [Validation](validation-1.6.md) |
| 1.5 | [Validation](validation-1.5.md) |
| 1.4 and earlier | [Historical report](validation-1.4.md) |

Source and binary versions are checked by the release scripts. Current machine-readable files can be refreshed by a new release validation; use Git history to recover prior evidence snapshots.
