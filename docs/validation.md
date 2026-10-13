# Current validation

See [Flux 2.0 validation](validation-2.0.md) and the [machine-readable release record](validation/release.json) for the current local build. Its core, frontend, browser and installer checks were executed for this upgrade. [Flux 1.7 validation](validation-1.7.md), [HUD repair checks](validation-hud-1.7.md), [Flux 1.6 validation](validation-1.6.md) and [Flux 1.5 validation](validation-1.5.md) are historical records. The report below is a historical Flux 1.4 record.

# Flux 1.4 doğrulama raporu

12 Ekim 2026, Windows x64. Node 24.18, izole Rust 1.99 / LLVM MinGW UCRT; FFmpeg 9.0.2 ve libxml2 2.15.4 sabit kaynaklardan derlendi. Medya testleri dağıtıma giren küçük FFmpeg/FFprobe ile yürütüldü; test videolarını üretmek için ayrı tam FFmpeg kullanıldı. Medya testleri zorunlu etkinleştirildi, atlanmadı.

## Otomatik denetimler

| Denetim | Sonuç |
| --- | --- |
| HTTP indirme entegrasyonları | 22 geçti |
| Onay/iptal/toplu hedef klasörü | 3 geçti |
| Video/ses eşleştirme ve çözünürlük tekilleştirme | 2 geçti |
| Otomatik bağlantı politikası | 1 geçti |
| Paralel medya, kesilmiş bağlantı, bozuk önbellek ve değişmiş sürüm | 3 geçti |
| Oturum/şifreli depo entegrasyonları | 2 geçti |
| IPC entegrasyonları | 4 geçti |
| HLS/DASH medya entegrasyonları | 2 geçti |
| TS, ayrı ses, sürdürme, MP3, zararlı manifest, .jpg parçalı/uzantısız HLS | 7 geçti |
| MV3 outbox/bridge/background/HUD DOM testleri | 23 geçti |
| Clippy, workspace/all-targets/all-features, -D warnings | Geçti |
| Rust format | Geçti |
| TypeScript strict ve Vite üretim build | Geçti |
| i18n | 20 katalog × 120 ortak, dolu anahtar |
| npm audit --omit=dev | 0 bildirilen açık |

HTTP kapsamı: doğru byte birleştirme, 32 bağlantı sınırı, dinamik yavaş parça bölme, kesinti/429/5xx/Retry-After, yanlış/ihmal edilmiş Range, güçlü doğrulayıcı olmadan güvenli tek akış, bozuk checkpoint, ETag değişimi, boş dosya, yanlış SHA, dosya çakışması, UUID idempotence, süreç kilidi, yeniden açılış, global/dosya hız sınırı, öncelik, zamanlı başlatma/durdurma, batch ve kapsamlı joker listeleme.

1.2 paralel medya testleri 12 MiB kaynak üzerinde eşzamanlı Range isteklerini, bozulmuş bir parçanın yeniden indirilmesini, diğer doğrulanmış parçaların yeniden kullanılmasını, değişmiş ETag ile bütün parçaların yenilenmesini ve yanlış Content-Range reddini doğrular. YouTube kaynaklarında sabit sayfa/format ile imzalı `itag/lmt/clen` sürümü kullanılır; uzunluğu eşleşmeyen veya sürüm bilgisi olmayan doğrulanmamış kaynaklar bu kurtarma yolunu kullanmaz.

Medya kapsamı: gerçek FFmpeg/FFprobe, video+ses HLS ve DASH, kimlik doğrulamalı HLS inceleme, sıralı TS, ayrı ses MKV, hız sınırlı kısmi cache'den byte range sürdürme ve uzak manifest'in yerel dosya okumasının reddi.

Eklenti testleri Chrome API sözleşmeleri için mock kullanır. Gerçek native süreç/taşıma ayrıca sınanmıştır: korelasyon, kopma, UUID ile worker restart, şifreli credential, kaynak doğrulama öncesi tarayıcı sahipliğinin korunması, iframe medya kapsamı, MIME keşfi, oturumdan çıkışta başlık temizliği, 20 katalog ve custom dil menüsü.

1.1 regresyonları: onaysız görev start/resume/scheduler ile başlayamaz; onay durumu yeniden açılışta korunur; ad/klasör değişimi atomiktir; iptal edilen ad serbest bırakılır; sonra indirme görevleri Paused kalır. Chrome kapatma/iptal veya kaynak 403 hatasında sürdürülür, doğrulamadan önce iptal edilmez. tabId=-1 isteklerinin Referer/Cookie/User-Agent bilgileri taşınır; sahipsiz duraklatmalar worker açılışında geri bırakılır. JSDOM üzerinde gerçek content.js ile HUD çarpısı, aynı sayfada kalıcı gizleme, MP3 bildirim sesleri/TS parçalarını dışlama, 1080p/720p seçenekleri ve YouTube'a ait sesin seçilmesi sınandı.

## Gerçek süreç ve kurulum

[Native entegrasyon JSON](validation/native-integration.json): release CLI ve gerçek native-host stdio + TCP broker, 20 eşzamanlı istek, 8 çerezle korunan indirme, aynı adın güvenli yeniden adlandırılması, UUID replay ve SHA-256. Açık credential SQLite/WAL içinde bulunmadı.

[Masaüstü entegrasyon JSON](validation/desktop-integration.json): kurulmuş masaüstü native host tarafından otomatik açıldı. 20 eşzamanlı istek ve 9 indirme doğrulandı. Dokuzuncu dosyada aktif aktarım sırasında --shutdown, checkpoint, otomatik yeniden açılış ve kaldığı byte'dan tamamlama sınandı. Bütün çıkışlar beklenen SHA-256 ile eşleşti.

[Yükleyici JSON](validation/installer.json): sessiz kurulum, Chrome/Edge/Chromium HKCU kaydı, doğru host manifest yolu, paket içi araç/DLL dosyaları, geliştirici araçları PATH'ten çıkarıldığında çalışma, kaldırma ve kullanıcı verisi koruma geçti. Test önceki kayıtları saklayıp geri yükledi; gerçek kullanıcı indirmelerini kullanmadı.

[HTTPS JSON](validation/https-download.json): GNOME resmi HTTPS sunucusundan libxml2 arşivi gerçek Range ile indirildi. 3.177.444 byte, 8 işçi, beklenen SHA-256: 98087fd181d9070724f3fbc65c7377db03038eb92bd882374daff44940138821.

## Arayüz

1.1 üretim frontend önizlemesi: menü/araç çubuğu/tablo düzeni, dil araması ve Türkçeye geçiş; konsol hatası yoktu. [Arayüz](images/desktop-1.1.png). 1.2'de gerçek Windows EXE ve WebView2 üzerinde native IPC ile formdan 32 bağlantıyla başlatma, ayrıntı penceresini dört kez açıp kapatma, ayar penceresi ve Otomatik varsayılanı sınandı. 96 MiB test dosyası beklenen SHA-256 ile tamamlandı. [Windows regresyon kaydı](validation/windows.json). Bu test kullanıcı Chrome oturumuna bağlanmaz; izole uygulama sürecinin yerel WebView2 test arabirimini kullanır.

[1.2 gerçek YouTube 1080p testi](validation/youtube-1080p.json): Kullanıcının sorun bildirdiği “Ali Ağaoğlu Nereye Kayboldu? | Para, Kadınlar ve Skandallar” videosundan altı tekil çözünürlük çözüldü. Geçersiz başlangıç adresi yenilendi; bir doğrulanmış medya parçası indikten sonra süreç kapatılıp açıldı, değişen imzalı adresle sürdürme tamamlandı. 8,04 saniyelik MP4'te 1080p H.264 görüntü/AAC ses, 8 saniyelik MP3'te tek MP3 ses akışı FFprobe ile doğrulandı. İki dosyanın SHA-256 değerleri rapordadır. Test gerçek native-host ve paket içindeki yt-dlp/Node/FFmpeg araçlarını kullanır. Eski Blender kaydı [önceki YouTube testidir](validation/youtube.json).

Aşağıdaki 500 görev, RTL ve eski arayüz görüntüleri 1.0 çalışmasının tarihli kayıtlarıdır; yeni pencere düzeninin testi olarak sayılmamıştır.

Gerçek Rust motoruna bağlı izole test arayüzünde 8 MB dosya eklendi ve tamamlandı. İkinci dosya duraklatıldı, sürdürüldü ve aynı beklenen SHA-256 ile tamamlandı. 500 görev batch olarak SQLite'a eklendi; toplam 502 görev için 10 DOM satırı oluştu. Arama, ayar kalıcılığı, 20 dil seçimi ve Arapça RTL görüldü. Üretim frontend'i temiz tarayıcı oturumunda ayrıca açıldı; konsolda hata/uyarı yoktu. Native masaüstünün görsel tıklama testi yerine aynı UI/engine komut hattının izole harness testi kullanıldı.

Kanıtlar: [tamamlanan dosyalar](images/completed.png), [duraklatma](images/download-paused.png), [500 görev](images/batch-500.png), [ayarlar](images/settings.png), [medya](images/media.png), [RTL](images/rtl.png), [üretim dil seçimi](images/production-language.png).

## Ölçüm

[Benchmark JSON](validation/benchmark.json), gerçek release CLI, 64 MiB, localhost sunucusunda bağlantı başına 64 KiB/5 ms. Başlatma, birleştirme ve SHA-256 dahil:

| İşçi | Süre | MiB/sn | Tek işçiye oran |
| --- | ---: | ---: | ---: |
| 1 | 16,659 s | 3,84 | 1× |
| 8 | 2,323 s | 27,55 | 7,17× |
| 32 | 1,302 s | 49,17 | 12,80× |

Bütün dosyalar 98dc891b284e4d84ac25b0c0a24fdbe39a7f0dbd643ad5e8aa06e02fc6258254 özetini verdi. Bu kontrollü bağlantı başına yavaşlatma senaryosudur; IDM ölçülmedi ve sonuç bir internet hız garantisi değildir.

## Yeniden çalıştırma ve kapsam

scripts/test-native.mjs, test-installer.ps1, benchmark.mjs ve Rust/Node testleri kaynak arşivindedir. Kaynaklar ile release dosyaları SHA256SUMS.txt üzerinden doğrulanabilir.

Windows x64 yerel çalıştırma doğrulandı. CI Windows/Linux çekirdek ve Windows masaüstü için tanımlandı; uzak CI çalıştırılmadı. Chrome içindeki gerçek eklenti yükleme/HUD görsel testi, Linux/macOS çalıştırma, ana dili konuşan kişilerle çeviri kalite incelemesi, uzun süreli WAN testi ve 120 FPS profili bu test kayıtlarının kapsamında değildir. Windows yükleyici Authenticode imzası taşımaz.

## 1.3 regresyonları

46 Rust ve 23 Node/DOM testi geçti. .jpg uzantılı TS parçaları hem FFprobe ile incelendi hem sesli MP4'e birleştirildi; uzantısız HLS adresinde yakalanan kind bilgisi sınandı. Sunucunun paralel medya Range isteğini bağlantı ortasında kesmesi yeniden denemeyle tam dosyayı verdi; diğer doğrulanmış parçalar korundu. İlk başlangıç zamanı kapatma/sürdürmede değişmedi; tamamlanma zamanı yayımlama sonrasında kaydedildi. Eski görevde yaklaşık dosya zamanına geçiş, boş HUD yüklenmesi, dışarı tıklamayla kapanma, gecikmiş yanıtın paneli yeniden açmaması, ön hazırlık/önbellek ve kalıcı masaüstü kabulünde Chrome cancel/erase kontrol edildi. Chrome API testleri taklit API olaylarıyla yürütüldü; kullanıcı Chrome oturumundaki gerçek etkileşim testi olarak sunulmaz.

1.3 kurulum paketiyle gerçek Native Messaging/TCP üzerinde 20 eşzamanlı istek ve 9 dosya indirildi. Aktif aktarımda kapatma/sürdürme, dört ayrı ayrıntı penceresini açıp kapatma, Otomatik ayarı, başlangıç zamanının ilerleme penceresinde görünmesi ve geçmiş tablosundaki gerçek başlangıç zamanının UTC kaydıyla eşleşmesi geçti. Kurulum/kaldırma, kayıtların geri yüklenmesi ve kullanıcı verisi koruması geçti. [1.3 regresyon kaydı](validation/regressions-1.3.json), [Windows pencere kaydı](validation/windows.json), [son paket kaydı](validation/release.json).

## 1.4 boyutu bilinmeyen medya

53 Rust testi, 23 MV3/DOM testi ve 6 arayüz veri testi geçti. Yeni medya ilerleme birimleri: boyutu bilinmeyen VOD için süre/ETA, canlı yayında sahte toplam oluşturmama, tekrar okuma ve bozuk denemelerde bayt tekilleştirme, DASH/klip süresi, seçilen yaprak manifest, önbelleği ağ hızına katmama. HTTP chunked, uzantısız/yanlış MIME HLS ve .jpg adlı TS parçalarıyla gerçek motor testi yüzde/ETA üretti; duraklatma/sürdürme ve son gerçek dosya boyutu doğrulandı.

Gerçek Windows EXE üzerinde aynı senaryo native IPC ile başlatıldı. Liste ve ilerleme penceresinde ≈ boyut, sıfırdan büyük yüzde, kalan süre, indirilen medya süresi ve parça sayısı görüldü. Sesli video FFprobe ile doğrulandı. [Medya pencere kaydı](validation/media-window.json), [görünüm](images/media-progress-1.4.png). Bu testin görüntüsü sentetik test filmidir; kullanıcı Chrome oturumundaki bir site testi olarak sunulmaz.

1.4 son yükleyiciyle 20 eşzamanlı native istek, 9 dosya aktarımı, aktif kapatma/sürdürme, dört ayrıntı penceresi ve tarih görünümü tekrar geçti. Paket içi FFmpeg/FFprobe ile Content-Length/MIME/uzantı bilgisi olmadan HLS filmi ayrı pencereden başlatıldı; yaklaşık boyut/yüzde, ETA, medya süresi ve parçalar hem listede hem pencerede doğrulandı. Duraklatıp sürdürme, 8,02 saniyelik görüntü+ses ve son gerçek boyut geçti. Son yükleyicinin SHA-256 değeri release.json ve installer.json içinde aynıdır.
