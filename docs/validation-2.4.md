# Flux 2.4.0 validation

Flux 2.4.0 was published from tag [`v2.4.0`](https://github.com/Hactor00/Flux/releases/tag/v2.4.0), commit `ee7af1baa93aea267c3c8091e9a4efb62214c92f`. Windows and Linux packages were built and tested in separate GitHub Actions runs, then assembled into one release after workflow provenance and SHA-256 checks passed.

## Automated checks

| Check | Result |
| --- | --- |
| Extension tests | 65 passed |
| Frontend tests | 19 passed |
| Localization catalogs, extension builds, strict TypeScript and production frontend build | Passed |
| Rust core library tests | 44 passed |
| Rust formatting and Clippy | Passed |
| Cross-platform CI matrix and desktop checks | Passed on the final run |

The automated results are recorded in [the v2.4 tag validation run](https://github.com/Hactor00/Flux/actions/runs/38806209487) and [the main-branch validation run](https://github.com/Hactor00/Flux/actions/runs/38806198863). The v2.4 tag run completed successfully after its Windows matrix job was rerun.

## Packaged platform checks

- [Windows release workflow](https://github.com/Hactor00/Flux/actions/runs/38806209521) built the installer and portable package and exercised the installed application, browser bridge, Chromium extension, Firefox compatibility packages and media handoff tests. The browser/package checks passed. Chromium reported a non-fatal WebView cleanup warning (`Error=1412`) after test cleanup; it did not fail the workflow or application checks.
- [Linux RPM workflow](https://github.com/Hactor00/Flux/actions/runs/38806209503) built the x86_64 RPM and installed it in Fedora 43. Dependency loading, desktop startup, HTTP payload checks, Chromium/Firefox native-host registration, watched torrent import, duplicate prevention and SQLite integrity checks passed.
- [Release assembly workflow](https://github.com/Hactor00/Flux/actions/runs/38811571879) verified the Windows/Linux source runs, package identities and release assets before publishing the complete release.

## Published package identities

The release page publishes [`SHA256SUMS.txt`](https://github.com/Hactor00/Flux/releases/download/v2.4.0/SHA256SUMS.txt) with hashes for every uploaded binary and metadata asset. Primary downloads:

| Asset | SHA-256 |
| --- | --- |
| `Flux_2.4.0_x64-setup.exe` | `a58507bebbf5099d09bf7253ab076e2932f589bb8a40c40ddb38faca8907d333` |
| `Flux-2.4.0-windows-x64.zip` | `698576f32c45982cb973c8478a49726b1968fe4ff86536d97e75f76a1a903716` |
| `Flux-2.4.0-linux-x86_64.rpm` | `416784419655cb5ae226338526fc2a9f3a9f42c45465bdcd0158e910a0267ef9` |
| `Flux-2.4.0-chrome.zip` | `687572067c879f7568a46817ccc4022802ae714ed3804db9fe0ae40b60b77739` |
| `Flux-2.4.0-firefox.zip` | `98a5c66d6708d9edacf6716089fbe2f9f91b8cb8089b4db5bf0566ab54f9d99c` |
| `Flux-2.4.0-firefox-120.zip` | `4fca34dcec6c79525dcee430f6ee231f87ac6fd7c52dcdfb7e0371c2d294ae97` |
| `Flux-2.4.0-source.zip` | `32f61d08d82b6a8ce25d2ecabca0fb030f3dd58d3d8df844eda9bd4edfab0280` |

The release assembler also compared the final installer with the tested Windows installer, portable binaries with their recorded hashes, and the RPM with its Fedora installation-test record.

## Scope and signing

The Linux package was tested on Fedora 43 x86_64; this does not establish compatibility with every RPM-based distribution. Browser media extraction still depends on streams the source site exposes to the signed-in user. This validation does not bypass DRM, CAPTCHA, authentication or unavailable opaque streams.

The Windows installer does not carry an Authenticode publisher certificate because production SignPath signing is not configured yet. The updater signature is present and serves a different integrity check. The RPM is not RPM-signed. Package hashes are published for independent verification.

---

# Flux 2.4.0 doğrulaması

Flux 2.4.0, [`v2.4.0`](https://github.com/Hactor00/Flux/releases/tag/v2.4.0) etiketi ve `ee7af1baa93aea267c3c8091e9a4efb62214c92f` commit'i üzerinden yayımlandı. Windows ve Linux paketleri ayrı GitHub Actions çalıştırmalarında derlenip test edildi; kaynak ve SHA-256 kontrollerinden sonra aynı sürümde birleştirildi.

## Otomatik kontroller

| Kontrol | Sonuç |
| --- | --- |
| Eklenti testleri | 65 geçti |
| Arayüz testleri | 19 geçti |
| Dil katalogları, eklenti derlemesi, katı TypeScript ve üretim arayüz derlemesi | Geçti |
| Rust çekirdek kütüphane testleri | 44 geçti |
| Rust biçimlendirme ve Clippy | Geçti |
| Platformlar arası CI matrisi ve masaüstü kontrolleri | Son çalıştırmada geçti |

Sonuçlar [v2.4 etiket doğrulama çalıştırmasında](https://github.com/Hactor00/Flux/actions/runs/38806209487) ve [ana dal doğrulama çalıştırmasında](https://github.com/Hactor00/Flux/actions/runs/38806198863) bulunur. v2.4 etiket çalıştırması, Windows matris işi yeniden çalıştırıldıktan sonra başarıyla tamamlandı.

## Kurulum paketleri

- [Windows yayın iş akışı](https://github.com/Hactor00/Flux/actions/runs/38806209521) kurulum ve taşınabilir paketleri oluşturdu; kurulu uygulama, tarayıcı köprüsü, Chromium eklentisi, Firefox uyumluluk paketleri ve medya aktarımı testlerini çalıştırdı. Bu kontroller geçti. Test temizliğinde Chromium kaynaklı `Error=1412` WebView uyarısı görüldü; uyarı iş akışını veya uygulama kontrollerini başarısız kılmadı.
- [Linux RPM iş akışı](https://github.com/Hactor00/Flux/actions/runs/38806209503) x86_64 RPM paketini derleyip Fedora 43'e kurdu. Bağımlılıkların yüklenmesi, masaüstünün açılması, HTTP içerik kontrolleri, Chromium/Firefox yerel köprü kaydı, izlenen torrent içe aktarımı, tekrar önleme ve SQLite bütünlüğü geçti.
- [Sürüm birleştirme iş akışı](https://github.com/Hactor00/Flux/actions/runs/38811571879) Windows/Linux kaynak çalıştırmalarını, paket kimliklerini ve yayın dosyalarını kontrol ettikten sonra sürümü yayımladı.

## Yayımlanan paketlerin kimlikleri

Yayın sayfasındaki [`SHA256SUMS.txt`](https://github.com/Hactor00/Flux/releases/download/v2.4.0/SHA256SUMS.txt), yüklenen tüm ikili ve metadata dosyalarının SHA-256 özetlerini içerir. Başlıca dosyalar:

| Dosya | SHA-256 |
| --- | --- |
| `Flux_2.4.0_x64-setup.exe` | `a58507bebbf5099d09bf7253ab076e2932f589bb8a40c40ddb38faca8907d333` |
| `Flux-2.4.0-windows-x64.zip` | `698576f32c45982cb973c8478a49726b1968fe4ff86536d97e75f76a1a903716` |
| `Flux-2.4.0-linux-x86_64.rpm` | `416784419655cb5ae226338526fc2a9f3a9f42c45465bdcd0158e910a0267ef9` |
| `Flux-2.4.0-chrome.zip` | `687572067c879f7568a46817ccc4022802ae714ed3804db9fe0ae40b60b77739` |
| `Flux-2.4.0-firefox.zip` | `98a5c66d6708d9edacf6716089fbe2f9f91b8cb8089b4db5bf0566ab54f9d99c` |
| `Flux-2.4.0-firefox-120.zip` | `4fca34dcec6c79525dcee430f6ee231f87ac6fd7c52dcdfb7e0371c2d294ae97` |
| `Flux-2.4.0-source.zip` | `32f61d08d82b6a8ce25d2ecabca0fb030f3dd58d3d8df844eda9bd4edfab0280` |

Sürüm birleştirme adımı ayrıca son kurulum dosyasını Windows kurulum testiyle, taşınabilir paketin ikili dosyalarını kayıtlı özetlerle ve RPM paketini Fedora kurulum testi kaydıyla karşılaştırdı.

## Kapsam ve imza durumu

Linux paketi Fedora 43 x86_64 üzerinde test edildi; bu sonuç her RPM tabanlı dağıtımda uyumluluk garantisi vermez. Tarayıcıdan medya çıkarma, kaynağın oturum açmış kullanıcıya sunduğu akışlara bağlıdır. Bu testler DRM, CAPTCHA, kimlik doğrulama veya erişilemeyen kapalı akışları aşmaz.

Üretim SignPath imzalama yapılandırması henüz tamamlanmadığından Windows kurulum dosyasında Authenticode yayıncı sertifikası yoktur. Uygulama içi güncelleyici imzası bulunur ve farklı bir bütünlük kontrolü sağlar. RPM imzalanmamıştır. Paketleri bağımsız doğrulamak için SHA-256 özetleri yayımlanmıştır.
