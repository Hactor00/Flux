<?php
declare(strict_types=1);
require __DIR__.'/lib.php';
secure_headers();
session_start_secure();
if (configuration()) { header('Location: index.php',true,303); exit; }
$message = '';
if ($_SERVER['REQUEST_METHOD'] === 'POST') {
    $tokenFile = private_directory().'/setup-token.txt';
    $token = is_file($tokenFile) ? trim((string)file_get_contents($tokenFile)) : '';
    $password = (string)($_POST['password'] ?? '');
    $hosting = trim((string)($_POST['hosting'] ?? ''));
    if (!csrf_valid() || strlen($token) < 48 || !hash_equals($token, (string)($_POST['token'] ?? ''))) { http_response_code(403); $message = 'Kurulum anahtarı veya oturum doğrulanamadı.'; }
    elseif (strlen($password) < 14 || strlen($password) > 200 || strlen($hosting) < 3 || strlen($hosting) > 200 || ($_POST['reviewed'] ?? '') !== 'yes') { http_response_code(400); $message = 'En az 14 karakterli parola, barındırma bilgisi ve yayın öncesi kontrol onayı gerekiyor.'; }
    else {
        $config = ['enabled'=>true,'secret'=>bin2hex(random_bytes(32)),'password_hash'=>password_hash($password,PASSWORD_DEFAULT),'hosting'=>$hosting,'created_at'=>gmdate(DATE_ATOM)];
        $file = private_directory().'/config.json';
        $handle = @fopen($file,'x');
        if (!$handle) { http_response_code(409); $message = 'Kurulum zaten yapılmış veya özel dizine yazılamıyor.'; }
        else {
            fwrite($handle,json_encode($config,JSON_PRETTY_PRINT|JSON_UNESCAPED_UNICODE|JSON_THROW_ON_ERROR)); fclose($handle); @chmod($file,0600);
            database(); unlink($tokenFile); session_regenerate_id(true); $_SESSION['admin']=time(); header('Location: index.php',true,303); exit;
        }
    }
}
page_start('İlk kurulum');
echo '<p>Bu adım tamamlanana kadar ölçüm kapalıdır. ZIP içindeki özel kurulum anahtarını kullan. Anahtar yalnızca bu kurulum için geçerlidir.</p>';
if ($message) echo '<p role="alert">'.escape($message).'</p>';
echo '<form method="post">'; csrf_field();
echo '<label>Kurulum anahtarı<input name="token" autocomplete="off" required></label><label>Yönetici parolası<input name="password" type="password" autocomplete="new-password" minlength="14" maxlength="200" required></label><label>Barındırma sağlayıcısı ve sunucu ülkesi<input name="hosting" maxlength="200" required placeholder="Sağlayıcı · ülke"></label><label class="check"><input type="checkbox" name="reviewed" value="yes" required> Gizlilik sayfasındaki iletişim, barındırma, aktarım ve saklama bilgilerini kontrol ettim. Sunucu günlüklerini sınırladım ve HTTPS etkin.</label><button>Kurulumu tamamla ve ölçümü aç</button></form>';
page_end();
