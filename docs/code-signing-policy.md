# Code signing policy

## Provider and current status

Flux is preparing to use the SignPath Foundation program for official Windows releases. The acknowledgment below applies after project approval and production signing are enabled:

> This project uses SignPath Foundation for code signing.

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

Foundation approval and production signing configuration are pending. Flux 2.4.3 Windows packages do not carry an Authenticode publisher certificate. The cryptographic signature used by Flux's updater is a separate mechanism and does not establish Foundation approval. Linux RPMs are distributed without an RPM signing certificate and include SHA-256 checksums.

## Project roles

- Maintainer, committer and reviewer: [Hactor00](https://github.com/Hactor00).
- Designated signing approver: [Hactor00](https://github.com/Hactor00), once the SignPath project is configured.

Changes from external contributors require maintainer review. Production signing requests require explicit approval by the designated approver; approval is not inferred from a successful build.

## Signing scope

Requests originate from the version-controlled [release workflow](../.github/workflows/release.yml). Signing covers Flux-owned application binaries and their official installation packages. Bundled third-party programs retain their own licenses and signatures; they are not represented as Flux-owned binaries. Build dependencies are pinned and checked before packaging. See [release verification and configuration](release.md#signpath-configuration).

## Privacy policy

[Flux privacy disclosure](https://github.com/Hactor00/Flux/blob/main/docs/browser-stores.md#privacy-disclosure-for-store-listings) describes local browser transfer, source connections, torrent traffic, optional Debrid APIs and GitHub update checks. Signing does not introduce an analytics endpoint or transfer the user's downloads, cookies or credentials to SignPath.

Project maintainers use GitHub and SignPath for release infrastructure: [GitHub privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement), [SignPath privacy policy](https://signpath.io/privacy-policy).

This policy follows the [SignPath Foundation conditions](https://signpath.org/terms.html). Preparation or acknowledgment does not imply application acceptance or an issued signing certificate.\n
