# Flux 2.4.1 validation

This version's release workflow builds and checks the exact 2.4.1 packages. It records the workflow run IDs, automated test results, package identities and SHA-256 hashes in `docs/validation/release.json` inside the source archive. The release's `release-info.json` links the Windows and Linux build runs. The 2.4.0 report remains historical and is not reused as evidence for this release.

The 2.4.1 checks cover the shared extension build and locale catalogs, frontend tests and production build, Rust tests and linting, Windows installer language selection and updater packaging, browser integration, media and torrent workflows, and the installed Fedora RPM. See the [2.4.1 release](https://github.com/Hactor00/Flux/releases/tag/v2.4.1) and its linked GitHub Actions runs for completed results.

Automated compatibility checks cannot establish that every third-party media site works. DRM, CAPTCHA, authentication challenges and opaque or short-lived media streams remain outside the release guarantee.

## Flux 2.4.1 doğrulaması

Bu sürümün yayın iş akışı, 2.4.1 paketlerinin kendisini derleyip denetler. İş akışı kimliklerini, otomatik test sonuçlarını, paket kimliklerini ve SHA-256 özetlerini kaynak arşivindeki `docs/validation/release.json` içinde kaydeder. Yayındaki `release-info.json`, Windows ve Linux derleme işlerini bağlar. 2.4.0 raporu geçmişe aittir; bu sürüm için kanıt olarak kullanılmaz.

2.4.1 kontrolleri ortak eklenti derlemesi ve dil kataloglarını, arayüz testleri ve üretim derlemesini, Rust testleri ve lint denetimini, Windows kurulum dili seçimini ve güncelleyici paketlemesini, tarayıcı bütünleşmesini, medya/torrent iş akışlarını ve Fedora RPM kurulumunu kapsar. Tamamlanan sonuçlar için [2.4.1 yayınını](https://github.com/Hactor00/Flux/releases/tag/v2.4.1) ve oradaki GitHub Actions bağlantılarını inceleyin.

Otomatik uyumluluk kontrolleri, tüm üçüncü taraf medya sitelerinin çalışacağını garanti etmez. DRM, CAPTCHA, oturum açma engelleri ve opak ya da kısa ömürlü medya akışları yayın garantisinin dışındadır.
