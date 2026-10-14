# Flux 2.2 teknik değişiklikleri

[English](release-2.2.md) · [Tüm özellikler](../README.tr.md) · [Yayın notları](releases/v2.2.0.md)

## Tarayıcıdan devralma

Kurulu ve taşınabilir masaüstü, yerel Native Messaging köprüsünü normal ve arka plan açılışlarında asenkron kaydeder/onarır. Değişmemiş dosya ve kayıt değerleri korunur; değişen manifest önce aynı klasörde geçici dosyaya yazılıp atomik değiştirilir. Kayıt hatası indirme motorunu kapatmaz; Ayarlar'daki tarayıcı bölümünde gösterilir. İlk kullanım bildiriminin kapatılması tercihlerden ayrı saklanır ve yalnız kullanıcının kapatma işlemiyle kaydedilir; arka plan açılışı bildirimi tüketmez. Eklentinin kurulması ve tarayıcı izinleri ayrı bir adımdır.

Chromium aktarımı masaüstü erişimi kontrol edilirken duraklatılır. Bilgiler tarayıcı iptalinden önce kalıcı, henüz gönderilemeyen bir outbox kaydına yazılır. İptal tamamlanınca kayıt etkinleştirilir; masaüstü önerisi teslim edilmeden dosya adı aşaması serbest bırakılmaz. Böylece tarayıcının dosya seçicisi Flux onayıyla yarışmaz. Eklenti arka planı yeniden başladığında ertelenen kayıt kurtarılır. İlk erişim kontrolünde Flux yoksa tarayıcı sahipliği korunur; kalıcı iptal sonrasındaki hatalar outbox içinde kalır.

Firefox HTTP GET dosya gezinmesini yanıt başlıklarındaki asenkron engelleyici dinleyiciyle yakalar. Açık tarayıcı API indirmeleri kalıcı iptal yolunu kullanır. `webRequestBlocking` yalnız Firefox manifestindedir; Chromium MV3 izinleri korunur.

Native Messaging keşif kaydı, masaüstü süreç kimliğini Windows ön plan yetkilendirmesi için taşır. Onay penceresi açıkça gösterilir ve odaklanır. Başka indirme yöneticisinin sistem kancası isteği daha önce yakalıyorsa ilgili tarayıcıda tek otomatik indirme sahibi kullanılmalıdır; Flux diğer uygulamanın ayarlarını değiştirmez.

## Sürdürme ve bütünlük

Geçerli `206` yanıtı ETag bulunmasa da Range desteğini etkinleştirir. ETag olmayan checkpoint'e ekleme yapılmadan kayıtlı başlangıç uzaktan okunur ve SHA-256 ile karşılaştırılır. Tamamlanmış parçalar da doğrulanır. Değişmiş başlangıç kayıtlı veriyi silmeden hata verir; karışık dosya yayımlanmaz.

Range olmayan sunucu başlangıcı tekrar gönderir; Flux doğrulanmış bölümü atıp kalanını ekler. Disk verisi ve görünen ilerleme korunur, fakat tekrar doğrulama ağ trafiği tüketir. İptal, genel/görev hız sınırı ve ağ süre aşımı bu doğrulamada da geçerlidir. Değişmiş güçlü doğrulayıcı veya toplam boyut yeni aktarım gerektirir. Range desteğini kaybeden parçalı aktarım doğrulanmış checkpoint'i koruyarak durur.

## Dil ve günlük arayüz

Yeni masaüstü profilleri işletim sistemi dilini 20 desteklenen dile eşler; bulunmayan dilde İngilizce kullanılır. Kaydedilmiş kullanıcı tercihi önceliklidir. Eklenti ilk seçimde tarayıcı arayüz dilini izler. Arapça/Urduca sağdan sola düzeni korunur.

Eklenti küçük penceresinin anahtarı açık çerçevelerde medya düğmelerini gösterir/gizler. Gizleme paneli de kapatır; yeniden açma çarpıyla gizlenmiş HUD'u geri getirir. Ayarlar üst menüden veya araç çubuğundan ayrı pencerede açılır.

## Paketleme ve güncellemeler

Kurulum, taşınabilir paket, üç eklenti hedefi, kaynak arşivi, SHA-256 ve imzalı güncelleme metaverisi 2.2.0 sürümünü taşır. Özel imza anahtarı depo ve paketlerin dışında kalır. Güncelleme imzası Authenticode yayıncı sertifikasından ayrıdır. Açılış kontrolü yeni sürüm bulursa indirme/kurulum öncesinde onay ister; Ayarlar'dan elle kontrol de mümkündür.

[Doğrulama](validation-2.2.md): 116 Rust, 59 eklenti ve 14 arayüz testi, toplam 189. Ayrı Chrome 155, Firefox 120/157, Windows pencere, medya/torrent ve kurulum regresyonlarının kanıtları `docs/validation/` altında saklanır.
