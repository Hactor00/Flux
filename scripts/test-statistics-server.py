"""Exercise the real local PHP HTTP endpoints with isolated private storage."""
from pathlib import Path
import http.cookiejar
import json
import os
import re
import secrets
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='flux-statistics-test-') as temp:
    temp = Path(temp)
    public = temp/'httpdocs'
    shutil.copytree(ROOT/'statistics-server',public/'usage')
    private = temp/'private'
    private.mkdir()
    token=secrets.token_hex(32)
    (private/'setup-token.txt').write_text(token)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
    environment={**os.environ,'FLUX_STATS_PRIVATE':str(private),'FLUX_STATS_TEST':'1'}
    server=subprocess.Popen(['php','-S',f'127.0.0.1:{port}','-t',str(public)],env=environment,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    base=f'http://127.0.0.1:{port}/usage/'
    opener=urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    def request(path,body=None,kind='application/x-www-form-urlencoded'):
        headers={'Content-Type':kind} if body is not None else {}
        try:
            with opener.open(urllib.request.Request(base+path,data=body,headers=headers),timeout=5) as response: return response.status,response.read().decode(),response.headers
        except urllib.error.HTTPError as error: return error.code,error.read().decode(),error.headers
    def form(path,**values): return request(path,urllib.parse.urlencode(values).encode())
    event={'action':'active','installation_id':'c4b7b992-b55e-46c8-b3fd-a9089227d123','version':'2.6.5','platform':'windows','notice_version':'2026-10-26'}
    try:
        for _ in range(40):
            try:
                status,body,headers=request('setup.php'); break
            except OSError: time.sleep(.1)
        else: raise RuntimeError('PHP did not start')
        csrf=re.search(r'name="csrf" value="([a-f0-9]+)"',body).group(1)
        assert request('collect.php',json.dumps(event).encode(),'application/json')[0]==503
        assert form('setup.php',token=token,password='correct test password 987',hosting='Test provider · local',reviewed='yes')[0]==403
        status,body,_=form('setup.php',csrf=csrf,token=token,password='correct test password 987',hosting='Test provider · local',reviewed='yes')
        assert status==200 and 'Aktif kurulumlar' in body
        assert not (private/'setup-token.txt').exists()
        assert request('collect.php')[0]==405
        assert request('collect.php',json.dumps(event).encode(),'application/json')[0]==204
        assert request('collect.php',json.dumps(event).encode(),'application/json')[0]==204
        assert request('collect.php',json.dumps({**event,'url':'https://secret.example'}).encode(),'application/json')[0]==400
        assert request('collect.php',b'x'*2049,'application/json')[0]==400
        status,body,headers=request('index.php')
        assert status==200 and 'windows' in body and '2.6.5' in body
        assert 'no-store' in headers['Cache-Control'] and "frame-ancestors 'none'" in headers['Content-Security-Policy']
        csrf=re.search(r'name="csrf" value="([a-f0-9]+)"',body).group(1)
        assert 'Yönetici girişi' in form('index.php',csrf=csrf,logout='1')[1]
        status,body,_=request('index.php'); assert 'windows' not in body
        csrf=re.search(r'name="csrf" value="([a-f0-9]+)"',body).group(1)
        assert form('index.php',csrf=csrf,password='wrong')[0]==403
        assert 'Aktif kurulumlar' in form('index.php',csrf=csrf,password='correct test password 987')[1]
        deletion={'action':'delete','installation_id':event['installation_id']}
        assert request('collect.php',json.dumps(deletion).encode(),'application/json')[0]==204
        assert 'Henüz veri yok' in request('index.php')[1]
        print('Passed real HTTP setup, disabled collector, CSRF, consent event validation, deduplication, private login/logout, security headers and deletion.')
    finally:
        server.terminate(); server.wait(timeout=10)
