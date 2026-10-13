import concurrent.futures
import hashlib
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import urllib.request

WORKSPACE = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(WORKSPACE / '.tools' / 'torrent-bootstrap'))
import zstandard

BASE = 'https://repo.msys2.org/mingw/ucrt64/'
PREFIX = 'mingw-w64-ucrt-x86_64-'


def metadata():
    database = urllib.request.urlopen(BASE + 'ucrt64.db', timeout=30).read()
    with zstandard.ZstdDecompressor().stream_reader(io.BytesIO(database)) as stream:
        archive = tarfile.open(fileobj=stream, mode='r|')
        result = {}
        for member in archive:
            if not member.name.endswith('/desc'):
                continue
            text = archive.extractfile(member).read().decode()
            fields = {}
            for part in text.strip().split('\n\n'):
                lines = part.splitlines()
                fields[lines[0].strip('%')] = lines[1:]
            result[fields['NAME'][0]] = fields
        return result


def plan(packages):
    selected = {}
    pending = [PREFIX + 'libtorrent-rasterbar', PREFIX + 'gcc']
    while pending:
        name = pending.pop().split('>=')[0].split('=')[0].split('>')[0]
        if name in selected:
            continue
        if name not in packages:
            raise RuntimeError('Missing runtime package: ' + name)
        entry = packages[name]
        selected[name] = {
            'name': name, 'version': entry['VERSION'][0],
            'file': entry['FILENAME'][0], 'sha256': entry['SHA256SUM'][0],
        }
        pending.extend(entry.get('DEPENDS', []))
    return list(selected.values())


def fetch(entry, destination):
    archives = WORKSPACE / '.tools' / 'torrent-archives'
    archives.mkdir(parents=True, exist_ok=True)
    path = archives / entry['file']
    if not path.exists():
        with urllib.request.urlopen(BASE + entry['file'], timeout=90) as response:
            path.write_bytes(response.read())
    if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
        raise RuntimeError('Runtime checksum mismatch: ' + entry['name'])
    subprocess.run(['tar', '-xf', str(path), '-C', str(destination), '--exclude=.BUILDINFO', '--exclude=.MTREE', '--exclude=.PKGINFO'], check=True)


def main():
    lock = WORKSPACE / 'native' / 'torrent' / 'windows-runtime.lock.json'
    if '--refresh-lock' in sys.argv:
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_text(json.dumps(plan(metadata()), indent=2) + '\n', encoding='utf-8')
        print('Pinned native torrent packages.')
        return
    entries = json.loads(lock.read_text(encoding='utf-8'))
    destination = WORKSPACE / '.tools' / 'torrent-native'
    destination.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for _ in pool.map(lambda entry: fetch(entry, destination), entries):
            pass
    print('Verified and extracted native torrent runtime packages.')


if __name__ == '__main__':
    main()
