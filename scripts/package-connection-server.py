#!/usr/bin/env python3
"""Bundle pinned desktop server executables. Never run them during packaging.

Windows uses the upstream release's published SHA-256. macOS/Linux compile the
exact upstream commit locally with a separate server toolchain, retaining the
app's Rust 1.75 contract. BEYONDREMOTE_SERVER_SOURCE may reuse that exact checkout.
No paid builder, hosted compilation or runtime download is required.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import zipfile

REVISION = '73523b31cfd25d77dee862e6fc9f5e1fb5e485ef'
VERSION = '1.1.16'
WINDOWS_SHA = 'b865a3a62fc8755b45480c508f1c4871c3338590408dda8c58c7e9c373b7adb0'
ROOT = Path(__file__).resolve().parents[1]

def run(*args, **kwargs):
    return subprocess.check_output(args, text=True, **kwargs).strip()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('bundle', type=Path)
    args = parser.parse_args()
    bundle = args.bundle.resolve()
    if not bundle.is_dir():
        parser.error('Build the app before packaging its server')
    system = platform.system()
    target = bundle / ('Contents/Resources/connection-server' if system == 'Darwin' else 'connection-server')
    target.mkdir(parents=True, exist_ok=True)
    if system == 'Windows':
        if platform.machine().lower() not in ('amd64', 'x86_64'):
            parser.error('This Windows server package requires x64')
        url = f'https://github.com/rustdesk/rustdesk-server/releases/download/{VERSION}/rustdesk-server-windows-x86_64-unsigned.zip'
        with tempfile.TemporaryDirectory(prefix='beyondremote-server-') as temp:
            archive = Path(temp) / 'server.zip'
            # The bundled Python runtime can lack Windows' issuer certificates.
            # Use the OS curl trust store, retain HTTPS verification, and then
            # verify the exact upstream archive hash before extracting anything.
            curl = Path(os.environ['SystemRoot']) / 'System32/curl.exe'
            subprocess.run([str(curl), '--fail', '--location', '--proto', '=https',
                            '--proto-redir', '=https', '--connect-timeout', '15',
                            '--max-time', '60', '--output', str(archive), url], check=True)
            if hashlib.sha256(archive.read_bytes()).hexdigest() != WINDOWS_SHA:
                raise RuntimeError('Server download hash mismatch')
            with zipfile.ZipFile(archive) as zip_file:
                for name in ('hbbs.exe', 'hbbr.exe'):
                    matches = [p for p in zip_file.namelist() if Path(p).name == name]
                    if len(matches) != 1:
                        raise RuntimeError('Unexpected server archive layout')
                    (target / name).write_bytes(zip_file.read(matches[0]))
        license_source = ROOT / 'LICENCE'
    else:
        source = Path(os.environ.get('BEYONDREMOTE_SERVER_SOURCE', ROOT / 'target/connection-server-source'))
        if not source.exists():
            source.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run(['git', 'clone', '--branch', VERSION, '--depth', '1', '--recurse-submodules', '--shallow-submodules',
                            'https://github.com/rustdesk/rustdesk-server.git', str(source)], check=True)
        if run('git', '-C', str(source), 'rev-parse', 'HEAD') != REVISION:
            raise RuntimeError('Unexpected server source revision')
        if run('git', '-C', str(source), 'status', '--porcelain', '--untracked-files=no'):
            raise RuntimeError('Server source contains unreviewed changes')
        subprocess.run(['cargo', '+1.98.0', 'build', '--locked', '--release', '--bin', 'hbbs', '--bin', 'hbbr'], cwd=source, check=True)
        for name in ('hbbs', 'hbbr'):
            shutil.copy2(source / 'target/release' / name, target / name)
            (target / name).chmod(0o755)
            if system == 'Darwin':
                subprocess.run(['codesign', '--force', '--sign', '-', str(target / name)], check=True)
        license_source = source / 'LICENSE'
    shutil.copy2(license_source, target / 'LICENSE-AGPL-3.0.txt')
    manifest = {'version': VERSION, 'source': 'https://github.com/rustdesk/rustdesk-server',
                'revision': REVISION, 'platform': system, 'architecture': platform.machine(),
                'files': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in target.iterdir() if p.is_file() and p.name != 'manifest.json'}}
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print('Packaged pinned desktop connection server:', target)

if __name__ == '__main__':
    main()
