# Flux 2.5.0 validation

Version 2.5.0 was tested locally before release preparation: 71 extension tests, 22 frontend tests and 130 Windows Rust tests passed. The Linux build passed 128 Rust tests and Clippy; its RPM was installed in Fedora 43 and passed desktop startup, SHA-256-verified HTTP download, native torrent loading, both browser identities, native messaging, watched-folder import, duplicate prevention and SQLite integrity checks.

Production Chromium UI modules were exercised across 20 languages. The production toolbar was checked with real pointer clicks, dragging and close actions in native dialogs, transformed custom modals and fullscreen containers. Firefox 120.0 and 157.0.1 integration checks passed against the Windows package. These fixtures do not prove compatibility with every live media provider.

The release pipeline independently builds and validates the Windows installer and Linux RPM. The source archive includes the Windows report as `docs/validation/release.json`; `release-info.json` identifies the successful platform workflow runs and package hashes. Prior releases' reports are retained as historical evidence only.

## Türkçe

2.5.0 için yerel eklenti, arayüz, Rust, gerçek Chromium/Firefox ve Fedora kurulum kontrolleri geçti. Yayın akışı ayrıca Windows kurulumunu ve Linux RPM'ini bağımsız olarak derleyip sınar. Genel kaynaktaki eski sürüm raporları yeni sürümün doğrulaması yerine kullanılmaz.
