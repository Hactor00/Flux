"""Create a private Plesk bundle without changing an existing deployed database."""
from pathlib import Path
import hashlib
import secrets
import zipfile

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / 'output' / '2.6.6'
DEST.mkdir(parents=True, exist_ok=True)
token_file = DEST / 'statistics-setup-key.txt'
if not token_file.exists():
    token_file.write_text(secrets.token_hex(32), encoding='utf-8')
archive = DEST / 'Flux-2.6.6-statistics-plesk-private.zip'
public = ['lib.php','collect.php','index.php','setup.php','prune.php','panel.css','.htaccess']
instructions = '''Flux 2.6.6 · İstatistik sunucusu kurulumu

Bu ZIP özeldir, GitHub'a veya herkese açık bir yere yüklemeyin.
httpdocs/usage klasörünü alan adının httpdocs klasörüne, private klasörünü
httpdocs ile aynı seviyeye yükleyin. private kesinlikle web kökünde olmamalı.
PHP 8.3+, PDO SQLite ve HTTPS gerekli. PHP'nin private/flux-statistics'e yazma izni olmalı.

https://fluxdownload.org/usage/setup.php adresini açın. Kurulum anahtarı:
private/flux-statistics/setup-token.txt dosyasındadır. En az 14 karakterli
yönetici parolası belirleyin. Barındırma ve gizlilik bilgilerini kontrol edin.
İlk kurulumdan sonra panel: https://fluxdownload.org/usage/
Kurulumdan önce veri toplanmaz. Sadece uygulamada izin verenler sayılır.

Site ZIP'ini ayrıca httpdocs'a yükleyin. Siteyi güncellerken usage klasörünü
ve private klasörünü silmeyin. Sonraki sürümlerde private dizinini yeni
kurulum dosyalarıyla değiştirmeyin; veritabanı, parola ve anahtarlar burada.
Günlük saklama temizliği için Plesk Zamanlanmış Görevler'den şu komutu ekleyin:
php /ALAN-ADI-TAM-YOLU/httpdocs/usage/prune.php

Ayrıntılar İngilizce README.md dosyasında. Sayılar kesin kişi sayısı değildir;
aynı kişinin birden fazla kurulumu olabilir, izin vermeyenler sayılmaz.
'''
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as package:
    for name in public:
        package.write(ROOT/'statistics-server'/name,'httpdocs/usage/'+name)
    package.writestr('private/flux-statistics/setup-token.txt',token_file.read_text(encoding='utf-8'))
    package.writestr('private/flux-statistics/.htaccess','Require all denied\n')
    package.writestr('KURULUM.txt',instructions)
    package.write(ROOT/'statistics-server'/'README.md','README.md')
with zipfile.ZipFile(archive) as package:
    assert package.testzip() is None
    assert not any('config.json' in name or '.sqlite' in name for name in package.namelist())
archive.with_suffix('.zip.sha256').write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'  '+archive.name+'\n',encoding='utf-8')
(DEST/'ISTATISTIK-KURULUM.txt').write_text(instructions,encoding='utf-8')
print('Created private server bundle:',archive)
