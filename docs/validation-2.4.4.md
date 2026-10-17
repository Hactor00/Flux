# Flux 2.4.4 validation

The updater regression renders the production `Updater` component with controlled desktop transport responses. It checks the sequence **Check for updates → Later → Check for updates** while closing the previous resource rejects with the reported ACL error. The replacement must remain visible, unmount must release it, and equal-version or absent updates must remain hidden. Dialog primitives and native transport are test doubles; this test does not claim to install an update.

The Windows installer validation runs the actual resource-close command from both the main and Settings WebViews using an intentionally unknown resource ID. The native resource table must reject the ID rather than the ACL rejecting the command. This confirms the packaged permissions without destroying any active resource. Publication requires this evidence in the current release report, along with the normal browser, installer, media and torrent checks.

The version-specific Windows report is included in the source archive as `docs/validation/release.json`; `release-info.json` identifies successful platform builds. The Linux package workflow installs the RPM on Fedora 43. Results from older releases are not treated as 2.4.4 package validation.

## Türkçe

Güncelleme arayüz testi, gerçek `Updater` bileşenini kontrollü masaüstü yanıtlarıyla çalıştırır. Önceki kaydı kapatma işlemi bildirilen ACL hatasıyla başarısız olduğunda **Denetle → Daha sonra → yeniden denetle** akışında yeni sürümün görünmesini, pencere kapanınca kaydın bırakılmasını ve aynı sürümün tekrar sunulmamasını kontrol eder. Diyalog ve yerel iletişim bu testte taklittir; güncelleme kurulumu yapıldığı anlamına gelmez.

Windows kurulum testi, ana pencere ve Ayarlar WebView'ından gerçek kaynak kapatma komutuna bilinçli olarak var olmayan bir kimlik gönderir. Komutun izin engeline değil yerel kaynak tablosuna ulaşması gerekir. Aktif bir kaynak kapatılmadan paketlenmiş izinler doğrulanır. Bu kanıt ve olağan kurulum, tarayıcı, medya ve torrent kontrolleri sürüm raporunda bulunmadan yayın yapılmaz.

Kaynak arşivindeki `docs/validation/release.json` Windows sonuçlarını, `release-info.json` başarılı platform çalışmalarını içerir. Linux paketi Fedora 43 üzerinde kurularak sınanır. Önceki sürümlerin sonuçları 2.4.4 doğrulaması yerine kullanılmaz.
