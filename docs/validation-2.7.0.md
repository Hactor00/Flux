# Flux 2.7.0 validation

Release checks cover queue restart after Pause all, restrictive single-connection servers returning 503, rejected range probes, non-range interrupted transfers, ordinary four-second pauses, SHA-256 verification, and rolling speed measurements. Existing malformed-range and changed-object checks remain enabled.

Frontend checks cover all 20 languages, Chrome and Firefox packages, task-removal controls, compact windows and appearance settings. Native Windows and Linux release workflows validate installed packages. The published-upgrade check exercises the startup notice, repeated manual update checks, signed installation from 2.6.6, foreground reopening from a background-launched instance, and preservation of downloads and preferences.

An original URL from the reported third-party large-file failure was unavailable. The regression fixtures reproduce its connection restrictions and transient server failures; compatibility with that specific server requires its URL.
