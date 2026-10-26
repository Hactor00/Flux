<?php
declare(strict_types=1);
require __DIR__.'/lib.php';
function verify(bool $condition,string $message): void { if (!$condition) throw new RuntimeException($message); }
$directory=sys_get_temp_dir().'/flux-stats-'.bin2hex(random_bytes(8)); mkdir($directory); putenv('FLUX_STATS_PRIVATE='.$directory);
try {
    verify(configuration()===null,'Collection must start disabled');
    $db=database(); $config=['secret'=>str_repeat('ab',32)]; $now=1793010000;
    $event=['action'=>'active','installation_id'=>'b05d2e60-0e80-4499-8d62-62b222a06c09','version'=>'2.6.5','platform'=>'windows','notice_version'=>FLUX_NOTICE];
    collect($db,$config,$event,$now); collect($db,$config,$event,$now);
    verify(metrics($db,$now)[1]===1,'Repeated checks must deduplicate');
    collect($db,$config,$event,$now-86400);
    verify(metrics($db,$now)[7]===1,'Weekly count must deduplicate across days');
    $second=$event; $second['installation_id']='2ad49ec2-8498-4baf-bdf6-1243dc84c913'; $second['platform']='linux';
    collect($db,$config,$second,$now);
    verify(metrics($db,$now)[1]===2,'Two installations must be separate');
    $bad=$event; $bad['url']='https://private.example';
    try { validate_event($bad); throw new RuntimeException('Unexpected fields accepted'); } catch (InvalidArgumentException $expected) {}
    $bad=$event; $bad['installation_id']="' OR 1=1";
    try { validate_event($bad); throw new RuntimeException('Invalid identity accepted'); } catch (InvalidArgumentException $expected) {}
    $old=$event; $old['installation_id']='df5644f5-6082-4df6-b0b3-5f779ce87b11';
    collect($db,$config,$old,$now-40*86400); metrics($db,$now);
    verify((int)$db->query('SELECT COUNT(DISTINCT installation) FROM active')->fetchColumn()===2,'Old rows must expire');
    collect($db,$config,['action'=>'delete','installation_id'=>$event['installation_id']],$now);
    verify(metrics($db,$now)[30]===1,'Withdrawal must remove all retained days');
    $raw=(string)file_get_contents($directory.'/usage.sqlite');
    verify(!str_contains($raw,$event['installation_id']),'Database must not contain raw identity');
    echo "Passed: disabled default, daily/weekly deduplication, validation, expiry, deletion, HMAC storage.\n";
} finally {
    $db=null;
    foreach (glob($directory.'/*') as $file) unlink($file);
    rmdir($directory);
}
