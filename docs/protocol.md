# Yerel IPC protokolü

UTF-8 JSON öncesinde native byte order 32-bit uzunluk vardır. Windows little-endian kullanır. Flux iki yönde 256 KiB sınırı koyar. stdout yalnızca protokol, stderr tanılama içindir. [Chrome Native Messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging) kayıt/taşıma biçimi uygulanır.

İstek: version=1, request_id=UUID, command ve payload. Cevap: aynı version/request_id, ok, result veya error. Eşzamanlı cevap sırası değişebilir; UUID ile eşleştirilir.

| Komut | Girdi ve sonuç |
| --- | --- |
| add | DownloadRequest → kalıcı queued Task |
| propose | DownloadRequest → awaiting_approval Task; kullanıcı onayı beklenir |
| resolve | YouTube sayfa URL'si → başlık, süre ve gerçek video/ses formatları |
| task | id → güncel Task |
| pause | id → kontrol noktası oluşturulmuş Task |
| resume | id → scheduler kuyruğundaki Task |
| list | İlk sınırlı görev sayfası |
| list_page | offset, limit=1..50 → tasks, total, next_offset, has_more |
| inspect | url, referer, protected_credentials → MediaInfo |
| protect | credentials → şifreli zarf; yalnızca native host işler |

DownloadRequest: url, filename, connections=0..32 (0=Otomatik, varsayılan), speed_limit, expected_sha256, priority=-10..10, start_at/stop_at epoch saniye, media, referer, auto_rename, protected_credentials. media: kind=file/hls/dash (isteğe bağlı), container=mp4/mkv/mp3, video_stream, audio_stream, audio_url, duration_seconds, segment_urls (en fazla 500), provider. provider: desteklenen video sayfasının url'si, format_id ve height. Bu kayıt her başlatmada yeni imzalı video/ses adresleriyle çözülür; hata yenilemesi en fazla bir kez yapılır.

Task: id, created_at/started_at/finished_at (UTC epoch saniye), timestamp_estimated, request, status, total, completed, etag, ranged, sha256, error, destination, source_validated. Private credential alanları çıkarılır. source_validated gerçek gövdenin kabulüdür; medya için mux çıktısı da gerekir. Tam dosyanın özeti sha256 içindedir.

list_page byte bütçesi yüzünden limit'ten az görev döndürebilir; next_offset kullanılır. 70 büyük URL'li görev testte kayıpsız sayfalandı.

Host tam extension origin'ini allowed-origins.json ile doğrular. %LOCALAPPDATA%/Flux/broker.json dosyasından rastgele port/token okur; TCP zarfı token+request'tir. Token ACL ile korunur ve eklenti/frontend'e gönderilmez. Masaüstü kapalıysa yanındaki flux-desktop.exe --background açılır; başlangıç kilidi ve 15 saniye broker bekleme vardır. Protect masaüstü açmayı gerektirmez; normal istek deadline'ı 120 saniyedir.

UUID native gönderimden önce durable outbox'a kaydedilir. Credential önce Protect ile mühürlenir; retry girdiyi değiştirmez. Eklenti propose gönderir. Onay bekleyen görev aynı UUID/girdiyle yeniden okunur; onay/iptal sonrasında orijinal yakalama URL'siyle replay güncel görevi döndürür ve yeni görev oluşturmaz. Düzenlenmiş ad/hedef/YouTube formatı geri çevrilmez.

approve_download/reject_download yalnızca yerel Tauri komutlarıdır. Onaysız görev scheduler, resume veya start ile çalıştırılamaz. Kalıcı propose görevi kabul edildiğinde Chrome cancel/erase uygulanır; onay penceresi iptali veya daha sonraki kaynak hatası Chrome'u yeniden başlatmaz. Masaüstü ilk aktarımda erişilemezse sahipsiz aktarım Chrome'a geri verilir. onCreated üzerinden mümkün olan en erken pause çağrılır. onDeterminingFilename, pause tamamlanana kadar suggest çağrısını bekletir; her durumda bir kez çağırır. Kalıcı browserDrafts kaydı, outbox'a ulaşmamış duraklatmaları worker açılışında geri bırakır. [Chrome downloads API sözleşmesi](https://developer.chrome.com/docs/extensions/reference/api/downloads).

Host io.flux.download, extension iheejmamcoeigoallciifjminjnnnlbj. Windows Chrome/Edge/Chromium HKCU NativeMessagingHosts anahtarları manifest'e yönlenir. Kaldırma kurulum sahipliğini denetler; başka kurulum kaydını silmez. Ağ gözlemcisi [Chrome webRequest](https://developer.chrome.com/docs/extensions/reference/api/webRequest) nonblocking MV3 API'sini kullanır.

1.4 Task ekleri: total_estimated boolean ve media_progress. Progress olayları total_estimated ve media alanlarını taşır. MediaProgress: duration_ms, processed_ms, segments_total, segments_completed, percent, eta_seconds, phase=downloading/processing/live. Bilinmeyen değerler null olabilir. started_at/finished_at ile medya süreleri farklı amaçlar taşır. Yaklaşık değer UI'da ≈ ile işaretlenir. Request biçimi ve protokol sürümü 1 korunur.
