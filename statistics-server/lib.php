<?php
declare(strict_types=1);

const FLUX_NOTICE = '2026-10-26';
const FLUX_RETENTION_DAYS = 35;

function private_directory(): string {
    return getenv('FLUX_STATS_PRIVATE') ?: dirname(__DIR__, 2) . '/private/flux-statistics';
}
function configuration(): ?array {
    $path = private_directory() . '/config.json';
    if (!is_file($path)) return null;
    $value = json_decode((string)file_get_contents($path), true, 16, JSON_THROW_ON_ERROR);
    return is_array($value) && !empty($value['enabled']) ? $value : null;
}
function database(): PDO {
    $directory = private_directory();
    if (!is_dir($directory)) throw new RuntimeException('Private directory is missing');
    $db = new PDO('sqlite:' . $directory . '/usage.sqlite', null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
    $db->exec('PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL;');
    $db->exec('CREATE TABLE IF NOT EXISTS active (installation TEXT NOT NULL, day TEXT NOT NULL, version TEXT NOT NULL, platform TEXT NOT NULL, notice TEXT NOT NULL, PRIMARY KEY(installation,day));
        CREATE TABLE IF NOT EXISTS attempts (key TEXT PRIMARY KEY, starts INTEGER NOT NULL, count INTEGER NOT NULL);');
    return $db;
}
function validate_event(array $event): array {
    $action = $event['action'] ?? null;
    $id = $event['installation_id'] ?? null;
    if (!in_array($action, ['active', 'delete'], true) || !is_string($id) || !preg_match('/\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z/i', $id)) throw new InvalidArgumentException('Invalid event');
    $allowed = $action === 'active' ? ['action','installation_id','version','platform','notice_version'] : ['action','installation_id'];
    if (array_diff(array_keys($event), $allowed)) throw new InvalidArgumentException('Unexpected fields');
    if ($action === 'active' && (!is_string($event['version'] ?? null) || !preg_match('/\A\d{1,3}\.\d{1,3}\.\d{1,3}\z/', $event['version']) || !in_array($event['platform'] ?? null, ['windows','linux','macos'], true) || ($event['notice_version'] ?? null) !== FLUX_NOTICE)) throw new InvalidArgumentException('Invalid version, platform or notice');
    $event['installation_id'] = strtolower($id);
    return $event;
}
function collect(PDO $db, array $config, array $event, ?int $now = null): void {
    $event = validate_event($event);
    $now ??= time();
    $key = hash_hmac('sha256', $event['installation_id'], $config['secret']);
    $db->beginTransaction();
    try {
        $db->prepare('DELETE FROM active WHERE day < ?')->execute([gmdate('Y-m-d', $now - FLUX_RETENTION_DAYS * 86400)]);
        if ($event['action'] === 'delete') {
            $db->prepare('DELETE FROM active WHERE installation = ?')->execute([$key]);
        } else {
            $db->prepare('INSERT INTO active(installation,day,version,platform,notice) VALUES(?,?,?,?,?) ON CONFLICT(installation,day) DO UPDATE SET version=excluded.version,platform=excluded.platform,notice=excluded.notice')
                ->execute([$key,gmdate('Y-m-d',$now),$event['version'],$event['platform'],$event['notice_version']]);
        }
        $db->commit();
    } catch (Throwable $error) { if ($db->inTransaction()) $db->rollBack(); throw $error; }
}
function metrics(PDO $db, ?int $now = null): array {
    $now ??= time();
    $db->prepare('DELETE FROM active WHERE day < ?')->execute([gmdate('Y-m-d', $now - FLUX_RETENTION_DAYS * 86400)]);
    $result = [];
    foreach ([1,7,30] as $days) {
        $query = $db->prepare('SELECT COUNT(DISTINCT installation) FROM active WHERE day >= ?');
        $query->execute([gmdate('Y-m-d', $now - ($days - 1) * 86400)]);
        $result[$days] = (int)$query->fetchColumn();
    }
    foreach (['version','platform'] as $column) {
        $query = $db->prepare("SELECT $column AS label, COUNT(DISTINCT installation) AS total FROM active WHERE day >= ? GROUP BY $column ORDER BY total DESC");
        $query->execute([gmdate('Y-m-d', $now - 29 * 86400)]);
        $result[$column] = $query->fetchAll(PDO::FETCH_ASSOC);
    }
    $query = $db->prepare('SELECT day AS label,COUNT(DISTINCT installation) AS total FROM active WHERE day >= ? GROUP BY day ORDER BY day DESC');
    $query->execute([gmdate('Y-m-d',$now-29*86400)]);
    $result['daily'] = $query->fetchAll(PDO::FETCH_ASSOC);
    return $result;
}
function escape(string $value): string { return htmlspecialchars($value, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8'); }
function secure_headers(): void {
    header('Cache-Control: no-store, private');
    header('X-Content-Type-Options: nosniff');
    header('X-Frame-Options: DENY');
    header('Referrer-Policy: no-referrer');
    header("Content-Security-Policy: default-src 'none'; style-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'");
}
function session_start_secure(): void {
    $secure = !empty($_SERVER['HTTPS']) && $_SERVER['HTTPS'] !== 'off';
    if (!$secure && getenv('FLUX_STATS_TEST') !== '1') { http_response_code(400); exit('HTTPS is required.'); }
    ini_set('session.use_strict_mode','1');
    session_name('FluxStatistics');
    session_set_cookie_params(['lifetime'=>0,'path'=>'/usage/','secure'=>$secure,'httponly'=>true,'samesite'=>'Strict']);
    session_start();
    if (!isset($_SESSION['csrf'])) $_SESSION['csrf'] = bin2hex(random_bytes(32));
}
function csrf_valid(): bool { return is_string($_POST['csrf'] ?? null) && hash_equals($_SESSION['csrf'], $_POST['csrf']); }
function page_start(string $title): void {
    echo '<!doctype html><html lang="tr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>'.escape($title).' · Flux</title><link rel="stylesheet" href="panel.css"><main><header><a href="index.php">↓ Flux</a><span>Kullanım istatistikleri</span></header><h1>'.escape($title).'</h1>';
}
function page_end(): void { echo '<footer>Yalnızca izin veren kurulumlar · UTC günleri · 35 gün saklama</footer></main></html>'; }
function csrf_field(): void { echo '<input type="hidden" name="csrf" value="'.escape($_SESSION['csrf']).'">'; }
