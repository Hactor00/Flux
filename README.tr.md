# Flux

![Flux indirme yöneticisi](docs/images/flux-banner.svg)

[English](README.md) · **Türkçe**

Flux; Rust indirme motoru, ayrı masaüstü pencereleri ve Chrome/Firefox eklentileriyle çalışan, açık kaynaklı bir Windows indirme yöneticisidir.

**Dosyalarını indir. Erişilebilir medyayı yakala. Kuyruğunu yönet.**

Paralel indirme, tarayıcıdan aktarım, medya dönüştürme ve yerel indirme geçmişi tek uygulamada buluşur. Flux adı “akış” fikrinden gelir: tarayıcıdaki dosya ve medyanın indirme kuyruğuna akması.

## İndirme paketleri

**1.7.0**, **Windows x64** içindir. Yayın tamamlandığında aşağıdaki dosyalar deponun GitHub Releases bölümünde bulunur.

| Paket                        | Kullanımı                                               |
| ---------------------------- | ------------------------------------------------------- |
| `Flux_1.7.0_x64-setup.exe`   | Tarayıcı köprüsüyle birlikte kullanıcı hesabına kurulum |
| `Flux-1.7.0-windows-x64.zip` | Taşınabilir uygulama ve medya araçları                  |
| `Flux-1.7.0-chrome.zip`      | Chromium tabanlı tarayıcılar için eklenti               |
| `Flux-1.7.0-firefox.zip`     | Firefox 140 ve sonrası için eklenti                     |
| `Flux-1.7.0-firefox-120.zip` | Firefox 120–139 için eklenti                            |
| `Flux-1.7.0-source.zip`      | Kaynak kod, medya lisansları ve bildirimleri            |

Mevcut yerel paketler kod imzası taşımıyor; Windows yayıncı veya SmartScreen uyarısı gösterebilir. İmza süreci [yayın belgelerinde](docs/release.md) açıklanır. Firefox eklentisinin standart Firefox'ta kalıcı kurulumu Mozilla imzası gerektirir.

## Özellikler

| Alan            | İşlevler                                                                                                                                                    |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| İndirme motoru  | Otomatik bağlantı seçimi veya dosya başına 1–32 bağlantı; HTTP Range, yavaş parçaların dinamik bölünmesi, sınırlı yeniden deneme, SHA-256 bütünlük kontrolü |
| Kalıcı geçmiş   | SQLite; eklenme, ilk başlatma ve tamamlanma zamanları; duraklatma ve sürdürme                                                                               |
| Kuyruk          | Öncelik, zamanlama, toplam/dosya hız sınırı, 500 bağlantıya kadar toplu ekleme ve joker bağlantı genişletme                                                 |
| Video ve ses    | HLS, DASH ve sıralı TS; MP4/MKV video, MP3 ses ve kayıpsız MKA ses çıkarma                                                                                  |
| Medya araçları  | Altyazı gömme veya SRT çıkarma, bölüm bilgileri ve iki geçişli ses normalizasyonu                                                                           |
| Kaynak seçimi   | YouTube sayfasından gerçek başlık ve çözünürlük başına tek seçenek; diğer sitelerde yakalanan erişilebilir HTTP(S) medya kaynakları                         |
| Ağ              | HTTP/3 ve TCP'ye geri dönüş; desteklenen kaynaklarda özgün tarayıcı oturumuyla süresi dolmuş adres yenileme                                                 |
| Masaüstü        | Ayrı indirme onayı, ayar ve ilerleme pencereleri; sanallaştırılmış tablo, arama, sıralama ve çoklu seçim                                                    |
| Dosya yönetimi  | Tamamlanan dosyaları Windows'a sürükleme; isteğe bağlı kategori klasörleri                                                                                  |
| Günlük kullanım | İsteğe bağlı pano izleme, sessiz kuyruğa ekleme ve Windows açılışında başlatma                                                                              |
| Kuyruk sonu     | İptal edilebilir 30 saniyelik sayımdan sonra uygulamadan çıkma, uyku, hazırda bekletme veya kapatma                                                         |
| Tarayıcı        | Chrome/Chromium ve Firefox; sağ tıkla aktarım, normal indirmeyi devralma ve hareketli video düğmesi                                                         |
| Dil             | 20 dil, 162 ortak çeviri anahtarı, aranabilir dil seçici; Arapça ve Urduca için sağdan sola düzen                                                           |

## Kurulum

Windows kurulum dosyasını çalıştır. Kurulum kullanıcı hesabına yapılır ve Chrome, Edge, Chromium ile Firefox için yerel mesajlaşma köprüsünü kaydeder. FFmpeg, FFprobe, yt-dlp ve Node paket içindedir. WebView2 yoksa Microsoft kurulum aracı kullanılır; bu aşama internet bağlantısı gerektirir.

Chrome'da `chrome://extensions`, Edge'de `edge://extensions` sayfasını aç. Geliştirici modunu etkinleştir ve kurulum klasöründeki `extension` klasörünü paketlenmemiş eklenti olarak yükle. Eklenti kimliği `iheejmamcoeigoallciifjminjnnnlbj` olur. Eklenti mağazalarında yayın ayrı bir işlemdir. Güncellemeden sonra eklentiyi yeniden yükle ve video sekmelerini yenile.

Firefox 140+ için `firefox.zip`, Firefox 120–139 için `firefox-120.zip` kullan. İmzasız yerel sürümler `about:debugging` üzerinden geçici yüklenir. Eklenti ayarlarında yerel Flux veri aktarımını aç; istendiğinde site erişimini kabul et. [Firefox kurulum belgesi](extension-firefox/README.md) ayrıntıları açıklar.

Taşınabilir ZIP doğrudan çalışır. Tarayıcı köprüsünü uygulamanın Ayarlar penceresinden kaydet. Uygulama verileri kullanıcının yerel uygulama verileri dizinindeki `Flux` klasöründe; varsayılan indirmeler `Downloads/Flux` klasöründe tutulur.

## Kullanım

Tarayıcıda bir indirme devralındığında ayrı Flux onay penceresi açılır. Dosya adını, hedef klasörü ve hemen başlayıp başlamayacağını seçebilirsin. Flux isteği kalıcı olarak kabul ettikten sonra tarayıcı aktarımı iptal edilir. İlk köprü aktarımı başarısızsa tarayıcı dosyanın sorumluluğunu korur.

Video düğmesini tutamacından sekmenin görünür alanındaki istediğin yere sürükle. Konum alan adına göre saklanır ve pencere boyutuna uyarlanır. Tutamaç odaktayken yön tuşları hareket ettirir; Home varsayılan video köşesine döndürür. Seçenek paneli yeterli alanın olduğu yönde açılır. Dışarı tıklamak paneli, çarpı ise o sayfa oturumu için bütün düğmeyi kapatır.

Kategori klasörleri isteğe bağlıdır. Varsayılan kategoriler arşiv, video, müzik, program ve belgedir; her hedef klasör değiştirilebilir. Elle seçtiğin hedef önceliklidir.

Pano izleme varsayılan olarak kapalıdır. Açıldığında onay penceresi veya sessiz kuyruğa ekleme seçilebilir. Sessiz mod kuyruğun etkinlik durumunu izler.

Kuyruk sonu eylemleri varsayılan olarak kapalıdır. Duraklatılmış, onay bekleyen veya başarısız görevler tamamlanma eylemini engeller. Geri sayım iptal edilebilir; yeni görev geldiğinde de iptal olur. İşletim sistemi eylemleri diğer uygulamaları zorla kapatmaz.

## Desteklenen diller

İngilizce, Mandarin Çincesi, Hintçe, İspanyolca, Standart Arapça, Fransızca, Bengalce, Portekizce, Rusça, Urduca, Endonezce, Almanca, Japonca, Nijerya Pidgini, Marathi, Telugu, Türkçe, Tamilce, Yue Çincesi ve Vietnamca.

## Geliştirme

Rust 1.99.0, Node 24.18.0 ve [Tauri önkoşulları](https://v2.tauri.app/start/prerequisites/) gerekir.

```powershell
npm ci
npm run tauri dev
```

`npm run dev` tarayıcı önizlemesidir; yerel indirme işlemleri masaüstü uygulamasıyla çalışır. Test arabirimleri üretim derlemesine dahil edilmez. [Katkı ve test yönergeleri](CONTRIBUTING.md), [mimari](docs/architecture.md) ve [IPC protokolü](docs/protocol.md) kaynak kodu açıklar.

## Doğrulama ve kapsam

[1.7 doğrulama belgesi](docs/validation-1.7.md), [son video düğmesi düzeltmeleri](docs/validation-hud-1.7.md) ve [JSON yayın kaydı](docs/validation/release.json) test kapsamını ve tarihlerini içerir. Son eklenti düzeltmesinde 39 eklenti ve 6 arayüz testi; Chrome'da kontrollü kaynakla gerçek panel yerleşimi; Firefox 120/157 ve yeni Windows kurulum paketi doğrulandı. Değişmeyen motorun önceki 82 test sonucu ayrı tarihli temel kayıt olarak korunur.

Eklenti HTTP(S) trafiğini gözlemler. Blob adresleri için alttaki ağ kaynağı gerekir. DRM, CAPTCHA, giriş doğrulamaları ve kapalı adres yenileme API'leri otomatik aşılmaz. Sonlu medyada yaklaşık boyut ve kalan süre `≈` ile gösterilir; canlı yayınlarda veya yeterli veri yokken kesin kalan süre hesaplanamaz. HTTP/3 arabirimi sabitlenmiş deneysel bir API kullanır ve TCP'ye geri döner.

## Lisans ve katkı

Flux kodu [MIT lisanslıdır](LICENSE). FFmpeg ve LAME LGPL, libxml2 MIT, yt-dlp Unlicense kapsamındadır. Node ve diğer bağımlılıkların kendi lisansları korunur; dağıtımlar lisans envanteri ve medya kaynaklarını içerir. SHA-256 dosya kimliğini doğrular; kod imzası veya zararlı yazılım taramasının yerini tutmaz.

Hataları deponun hata şablonuyla bildir. Sürümü ve tekrar oluşturma adımlarını ekle; kişisel adresleri, çerezleri ve yetkilendirme bilgilerini paylaşma. Güvenlik bildirimleri için [güvenlik politikasını](SECURITY.md) izle.
