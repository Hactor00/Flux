<?php
declare(strict_types=1);
require __DIR__.'/lib.php';
secure_headers();
session_start_secure();
$config = configuration();
if (!$config) { header('Location: setup.php',true,303); exit; }
$db = database();
if (isset($_SESSION['admin']) && time() - $_SESSION['admin'] > 1800) unset($_SESSION['admin']);
$message = '';
if ($_SERVER['REQUEST_METHOD'] === 'POST') {
    if (!csrf_valid()) { http_response_code(403); $message='Oturum doğrulanamadı. Sayfayı yenile.'; }
    elseif (isset($_POST['logout'])) { $_SESSION=[]; session_destroy(); header('Location: index.php',true,303); exit; }
    elseif (!isset($_SESSION['admin'])) {
        $key = hash_hmac('sha256', 'login:'.($_SERVER['REMOTE_ADDR'] ?? ''), $config['secret']);
        $db->prepare('DELETE FROM attempts WHERE starts < ?')->execute([time()-900]);
        $query=$db->prepare('SELECT count FROM attempts WHERE key=?'); $query->execute([$key]);
        if ((int)$query->fetchColumn() >= 6) { http_response_code(429); $message='Çok fazla deneme. 15 dakika sonra tekrar dene.'; }
        elseif (password_verify((string)($_POST['password'] ?? ''),$config['password_hash'])) {
            $db->prepare('DELETE FROM attempts WHERE key=?')->execute([$key]);
            session_regenerate_id(true); $_SESSION['admin']=time(); header('Location: index.php',true,303); exit;
        } else {
            $db->prepare('INSERT INTO attempts(key,starts,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=count+1')->execute([$key,time()]);
            http_response_code(403); $message='Giriş doğrulanamadı.';
        }
    }
}
page_start(isset($_SESSION['admin']) ? 'Aktif kurulumlar' : 'Yönetici girişi');
if ($message) echo '<p role="alert">'.escape($message).'</p>';
if (!isset($_SESSION['admin'])) {
    echo '<form method="post">'; csrf_field(); echo '<label>Parola<input name="password" type="password" autocomplete="current-password" required></label><button>Giriş yap</button></form>'; page_end(); exit;
}
$_SESSION['admin']=time();
$stats=metrics($db);
echo '<p>Bu sayılar paylaşımı açan ayrı kurulumları gösterir. İndirme sayısı veya kesin kişi sayısı değildir; aynı kişinin iki cihazı iki kurulum sayılabilir. Paylaşımı kapalı kullanıcılar görünmez.</p><div class="metrics">';
foreach ([1=>'Bugün',7=>'Son 7 gün',30=>'Son 30 gün'] as $days=>$label) echo '<article><span>'.escape($label).'</span><strong>'.$stats[$days].'</strong></article>';
echo '</div>';
foreach (['daily'=>'Günlük etkinlik','version'=>'Sürümler · son 30 gün','platform'=>'İşletim sistemi · son 30 gün'] as $group=>$label) {
    echo '<section><h2>'.escape($label).'</h2><table><thead><tr><th>'.($group==='daily'?'Gün (UTC)':'Grup').'</th><th>Kurulum</th></tr></thead><tbody>';
    foreach ($stats[$group] as $row) echo '<tr><td>'.escape((string)$row['label']).'</td><td>'.(int)$row['total'].'</td></tr>';
    if (!$stats[$group]) echo '<tr><td colspan="2">Henüz veri yok.</td></tr>';
    echo '</tbody></table></section>';
}
echo '<p class="muted">Barındırma: '.escape($config['hosting']).'</p><form method="post">';csrf_field(); echo '<button class="secondary" name="logout" value="1">Çıkış yap</button></form>';page_end();
