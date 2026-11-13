#!/usr/bin/env python3
"""Package a BEYONDREMOTE_STAGING=1 build as a separately searchable Mac app.

Run build.py with BEYONDREMOTE_STAGING=1 first. This helper never modifies the
stable application or copies credentials/settings. Destination must not exist.
"""
import argparse
import hashlib
import json
import os
import pathlib
import plistlib
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=pathlib.Path)
parser.add_argument('destination', type=pathlib.Path)
parser.add_argument('--identity', help='SHA-1 fingerprint of the persistent Apple signing identity')
args = parser.parse_args()
identity = args.identity or os.environ.get('BEYONDREMOTE_SIGNING_IDENTITY')
if not identity:
    config = pathlib.Path.home() / '.config/beyond-studios/beyondremote-signing.json'
    if config.exists():
        identity = json.loads(config.read_text()).get('staging_identity')
if not identity or not re.fullmatch(r'[0-9A-Fa-f]{40}', identity):
    parser.error('A persistent Apple signing identity is required; ad-hoc signing loses privacy approvals across updates.')
identities = subprocess.run(['security', 'find-identity', '-v', '-p', 'codesigning'],
                            check=True, capture_output=True, text=True).stdout
if identity.upper() not in identities.upper():
    parser.error('The configured signing identity is unavailable. Refusing an ad-hoc fallback.')
# find-identity can report a locally valid identity whose certificate was revoked.
# Check the exact certificate before touching the output or installing anything.
certificates = subprocess.run(['security', 'find-certificate', '-a', '-p'],
                              check=True, capture_output=True, text=True).stdout
certificate = None
for pem in re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----', certificates, re.S):
    der = subprocess.run(['openssl', 'x509', '-outform', 'DER'], input=pem.encode(),
                         check=True, capture_output=True).stdout
    if hashlib.sha1(der).hexdigest().upper() == identity.upper():
        certificate = pem
        break
if certificate is None:
    parser.error('The configured signing certificate was not found.')
with tempfile.NamedTemporaryFile(suffix='.pem') as cert:
    cert.write(certificate.encode())
    cert.flush()
    verified = subprocess.run(['security', 'verify-cert', '-c', cert.name,
                               '-p', 'codeSign', '-R', 'ocsp'], capture_output=True, text=True)
    if verified.returncode:
        parser.error('Signing certificate trust/revocation check failed: ' + verified.stderr.strip())
if args.destination.exists():
    parser.error('Destination exists; preserve or move it before packaging.')
core = args.source / 'Contents/Frameworks/liblibrustdesk.dylib'
if b'BeyondRemote Staging' not in core.read_bytes():
    parser.error('Source is not a BEYONDREMOTE_STAGING=1 build.')
subprocess.run(['ditto', str(args.source), str(args.destination)], check=True)
plist_path = args.destination / 'Contents/Info.plist'
with plist_path.open('rb') as stream:
    info = plistlib.load(stream)
info.update(CFBundleName='BeyondRemote Staging',
            CFBundleDisplayName='BeyondRemote Staging',
            CFBundleIdentifier='us.beyondstudios.beyondremote.staging')
# Do not steal production rustdesk:// links.
for handler in info.get('CFBundleURLTypes', []):
    handler['CFBundleURLName'] = info['CFBundleIdentifier']
    handler['CFBundleURLSchemes'] = ['beyondremote-staging']
with plist_path.open('wb') as stream:
    plistlib.dump(info, stream)

# Sign nested code inside-out using the same identity. Never use --deep to sign:
# it can miss nonstandard helpers or give them the app's entitlements.
mach_magic = {bytes.fromhex(v) for v in ('feedface', 'cefaedfe', 'feedfacf', 'cffaedfe', 'cafebabe', 'bebafeca')}
for path in sorted(args.destination.rglob('*'), key=lambda p: len(p.parts), reverse=True):
    if path.is_symlink() or not path.is_file():
        continue
    with path.open('rb') as stream:
        is_code = stream.read(4) in mach_magic
    if is_code:
        subprocess.run(['codesign', '--force', '--options', 'runtime', '--timestamp=none',
                        '--sign', identity, str(path)], check=True)
for path in sorted(args.destination.rglob('*.framework'), key=lambda p: len(p.parts), reverse=True):
    if not path.is_symlink():
        subprocess.run(['codesign', '--force', '--options', 'runtime', '--timestamp=none',
                        '--sign', identity, str(path)], check=True)
entitlements = pathlib.Path(__file__).resolve().parents[2] / 'flutter/macos/Runner/Release.entitlements'
with entitlements.open('rb') as stream:
    values = plistlib.load(stream)
values['com.apple.security.cs.disable-library-validation'] = True
with tempfile.NamedTemporaryFile(suffix='.plist') as tmp:
    plistlib.dump(values, tmp)
    tmp.flush()
    subprocess.run(['codesign', '--force', '--options', 'runtime', '--entitlements', tmp.name,
                    '--timestamp=none', '--sign', identity, str(args.destination)], check=True)
subprocess.run(['codesign', '--verify', '--deep', '--strict', str(args.destination)], check=True)
subprocess.run(['codesign', '--verify', '-R', '=anchor apple generic', str(args.destination)], check=True)
print('Packaged BeyondRemote Staging with isolated app identity and config namespace.')
