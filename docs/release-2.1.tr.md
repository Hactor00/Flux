# Flux 2.1.0

Bu sürüm, torrent paylaşımını, bağlantı kurtarmayı ve uygulama içi güncellemeleri mevcut indirme yöneticisine ekler.

- **Torrent:** BitTorrent v2 ve hibrit torrentler; genel veya dosyaya özel paylaşım oranı, süre sınırı ve süresiz paylaşım. Paylaşım yeniden başlatmada sürer, indirme kuyruğunda yer işgal etmez.
- **Disk:** Windows'ta hizalı önbelleksiz yazım ve seyrek dosyalar, Linux'ta fallocate, macOS'ta F_PREALLOCATE. Parçalar hedef dosyaya yazılır; birleştirme yapılmaz. Dosya bütünlüğü SHA-256 ile doğrulanır.
- **Ağ:** Üç saniye veri gelmeyen bağlantı yenilenir; doğrulanan baytlardan devam edilir. TCP havuzu, canlılık denetimi ve soket tamponları yapılandırılır.
- **Dosya hizmetleri:** Real-Debrid, AllDebrid ve Premiumize hesapları ayarlardan eklenir. Anahtarlar şifreli saklanır; desteklenen dosya bağlantıları doğrudan indirme adreslerine dönüştürülür.
- **Medya:** Chrome ve Firefox entegrasyonunda yakalanan HLS/DASH manifestleri Rust ile çözümlenir. Kalite ve ses seçimi korunur; sağlayıcı çözümleyicisi gecikirse yedek yol kullanılır.
- **Güncelleme:** Açılışta arka planda kontrol, ayarlardan elle kontrol, sürüm notları ve ilerleme görünümü. İmza ve sürüm doğrulamasından sonra indirmeler kaydedilir; kullanıcı hesabına sessiz kurulum yapılır.
- **Arayüz:** Paylaşım görünümü, hesap ayarları ve güncelleme paneli. Yeni metinler 20 dile eklendi.

Windows'ta 111 Rust, 55 eklenti ve 12 arayüz testi geçti. Chrome 155, Firefox 120/157, ayrı masaüstü pencereleri ve temiz kurulum akışları doğrulandı. Torrent tamamlanmasında disk yazımını bekleme, çevrimdışı sürdürme ve bozuk parçayı onarma kontrolleri eklendi. Ayrıntılı kapsam `validation-2.1.md` dosyasındadır.

## Windows'ta derleme

Node.js, Rust ve Python kurulu olmalıdır. Bu çalışma alanındaki Rust araç zinciri için:

```powershell
npm ci
. ./scripts/use-local-rust.ps1
./scripts/build-torrent-runtime.ps1
npm test
cargo test --workspace --all-features
npm run tauri build -- --debug --no-bundle
```

Dağıtım paketi, güncelleme imzalama anahtarı ve hazırlanmış FFmpeg çalışma ortamı kullanır:

```powershell
./scripts/build-release.ps1 -UseLocalToolchain -RuntimePackage release/Flux-2.0.0-windows-x64
./scripts/finalize-public-release.ps1 -OutputDirectory deliverables/v2.1.0
```

Linux ve macOS'ta Boost, OpenSSL, CMake, Ninja ve pkg-config kurulduktan sonra `bash scripts/bootstrap-torrent-runtime.sh` çalıştırılır; ardından Rust testleri ve Tauri derlemesi yapılır.

Debrid için kendi hesabının API anahtarı gerekir. Windows sertifikası ve mağaza hesapları yayıncı tarafından sağlanır. GitHub Actions güncelleme imzaları `TAURI_SIGNING_PRIVATE_KEY` gizli değeriyle üretilir; özel anahtar kaynak koduna ve indirme paketlerine eklenmez.
