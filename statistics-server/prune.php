<?php
declare(strict_types=1);
if (PHP_SAPI !== 'cli') { http_response_code(404); exit; }
require __DIR__.'/lib.php';
$db=database();
$db->prepare('DELETE FROM active WHERE day < ?')->execute([gmdate('Y-m-d',time()-FLUX_RETENTION_DAYS*86400)]);
$db->prepare('DELETE FROM attempts WHERE starts < ?')->execute([time()-900]);
echo "Retention applied.\n";
