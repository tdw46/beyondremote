#!/usr/bin/env python3
"""Package a BEYONDREMOTE_STAGING=1 build as a separately searchable Mac app.

Run build.py with BEYONDREMOTE_STAGING=1 first. This helper never modifies the
stable application or copies credentials/settings. Destination must not exist.
"""
import argparse
import pathlib
import plistlib
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=pathlib.Path)
parser.add_argument('destination', type=pathlib.Path)
args = parser.parse_args()
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
entitlements = pathlib.Path(__file__).resolve().parents[2] / 'flutter/macos/Runner/Release.entitlements'
with entitlements.open('rb') as stream:
    values = plistlib.load(stream)
values['com.apple.security.cs.disable-library-validation'] = True
with tempfile.NamedTemporaryFile(suffix='.plist') as tmp:
    plistlib.dump(values, tmp)
    tmp.flush()
    subprocess.run(['codesign', '--force', '--options', 'runtime', '--entitlements', tmp.name,
                    '--sign', '-', str(args.destination)], check=True)
subprocess.run(['codesign', '--verify', '--deep', '--strict', str(args.destination)], check=True)
print('Packaged BeyondRemote Staging with isolated app identity and config namespace.')
