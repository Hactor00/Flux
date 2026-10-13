# Security policy

Flux receives authenticated browser requests and writes downloaded files. Treat browser session credentials, signed URLs and local database contents as private.

Report vulnerabilities through GitHub private vulnerability reporting when it is enabled. If that channel is unavailable, open an issue requesting a private contact method without including exploit details, credentials or private downloads.

Include the affected version, platform and a minimal reproduction using non-sensitive test data. Public logs must omit Cookie, Authorization, protected credential envelopes and personal filesystem paths.

SHA-256 verifies file identity. It does not certify that a file is harmless. Flux does not automatically bypass DRM, CAPTCHA or login challenges.
