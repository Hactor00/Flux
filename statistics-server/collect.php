<?php
declare(strict_types=1);
require __DIR__ . '/lib.php';
secure_headers();
header('Content-Type: application/json; charset=utf-8');
if ($_SERVER['REQUEST_METHOD'] !== 'POST') { header('Allow: POST'); http_response_code(405); exit('{"error":"method"}'); }
if (!str_starts_with(strtolower($_SERVER['CONTENT_TYPE'] ?? ''),'application/json') || (int)($_SERVER['CONTENT_LENGTH'] ?? 0) > 2048) { http_response_code(400); exit('{"error":"request"}'); }
try {
    $config = configuration();
    if (!$config) { http_response_code(503); exit('{"error":"not_ready"}'); }
    $body = file_get_contents('php://input', false, null, 0, 2049);
    if ($body === false || strlen($body) > 2048) throw new InvalidArgumentException('Request too large');
    $event = json_decode($body, true, 8, JSON_THROW_ON_ERROR);
    if (!is_array($event)) throw new InvalidArgumentException('Invalid event');
    collect(database(), $config, $event);
    http_response_code(204);
} catch (InvalidArgumentException | JsonException $error) { http_response_code(400); echo '{"error":"event"}'; }
catch (Throwable $error) { http_response_code(503); echo '{"error":"unavailable"}'; }
