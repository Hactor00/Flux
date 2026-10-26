#!/usr/bin/env bash
set -euo pipefail
# Avoid a stalled runner-specific Azure mirror; keep Ubuntu's signed repositories.
sudo python3 - <<'PY'
from pathlib import Path
paths = [Path('/etc/apt/sources.list'), *Path('/etc/apt').glob('*mirrors*.txt'), *Path('/etc/apt/sources.list.d').glob('*.list'), *Path('/etc/apt/sources.list.d').glob('*.sources')]
for path in paths:
    if not path.is_file():
        continue
    text = path.read_text()
    updated = text.replace('http://azure.archive.ubuntu.com/ubuntu', 'https://archive.ubuntu.com/ubuntu').replace('https://azure.archive.ubuntu.com/ubuntu', 'https://archive.ubuntu.com/ubuntu')
    if updated != text:
        path.write_text(updated)
PY
options=(-o Acquire::ForceIPv4=true -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o Acquire::Retries=3)
sudo apt-get "${options[@]}" update
sudo apt-get "${options[@]}" install -y --no-install-recommends "$@"
