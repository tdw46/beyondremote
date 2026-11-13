# Guided desktop setup acceptance

## Intended behavior

First-run checks are local and event-driven. Screen capture, device control and
keyboard monitoring on macOS each have a separate OS check and explicit action.
The app opens the relevant settings pane, rechecks on return, and never treats
clicking a button or cancelling a prompt as permission granted. Missing access
remains visible after dismissing the wizard.

Windows installation defaults to **System (recommended)**, with **Just for me**
and folder selection available. System installation retains OS elevation and
service requirements. Current-user installation copies a complete verified bundle
to an empty folder, creates selected user shortcuts, and installs no service or
machine firewall rules. Its limits are stated before installation. Staging
installation must not clean up the stable app's legacy folders or shared broker.

Wake setup distinguishes no physical adapter, unsupported driver, settings not
retained, adapter not armed, cancelled elevation and OS/driver failure. The
wizard provides power/adapter settings and a real sleep/wake acceptance step.
macOS only enables network wake on AC power and verifies read-back. No adapter
restart, automatic reboot, or battery-policy change is requested.

Server setup exposes app-specific firewall elevation and fixed settings links,
retains authenticated device/account checks, and explains router/ISP constraints.
It never disables the firewall, uploads router passwords, or claims UPnP solves
carrier-grade NAT. Success of local configuration is not internet acceptance.

## Frozen Mac remote window

The November 13 report describes working input with a stale Windows image. A
3-second process sample found active image/UI processing but an idle secondary
raster thread; even the local toolbar stopped repainting. This matches Flutter's
secondary-window lifecycle defect, not proof of a failed network or Windows
capture permission. Upstream references:

- https://github.com/flutter/flutter/issues/133533
- https://github.com/flutter/flutter/commit/272fe23e0e32d63fa6c34dbf9659411dcfabb970

The pinned Flutter 3.24 build now reconciles the lifecycle of each visible engine
on native focus/visibility events. Hidden/minimized windows are not forced to
render. There is no frame timer, server poll, transport change or codec downgrade.
Native reconnection showed updating Windows content and Tyler confirmed the streaming display updates are fixed. Repeated app-switch/minimize stress acceptance remains separate.

## Verification ledger

- Local setup widget tests: 3 passed, covering denial/cancellation, return from
  Settings, revocation, and avoiding repeat first-run prompts on configured apps.
- Flutter analyzer: no warnings/errors; four existing deprecation infos.
- Mac Swift/Flutter release build: passed.
- Wake result test: passed after the bounded diagnostic-pipe drain refinement.
- Final native Rust release build: passed; installed Mac bundle verified against 156 file hashes.
- Tyler confirmed Windows-to-Mac mouse and keyboard input after the authorized Device Control grant. Ad-hoc rebuilt apps must recheck OS permissions; previous grants are not assumed.
- Permission, wake and server dialogs now share clipped backdrop blur with crisp foreground text and an opaque high-contrast fallback. Focus/denial/revocation widget tests still pass (3); affected-file analyzer reports no issues. Native glass release build and visual checks passed for permission, wake and server dialogs on the installed Mac staging app. Use source-replacement blending inside the material layer to avoid a second sharp backdrop showing through.
- Windows native build, UAC cancellation/success, installed service and wake
  acceptance: pending trusted runner recovery. Mac checks are not Windows proof.
- Physical sleep/wake, phone, production API promotion and final cutover remain
  separate gates.

All setup status polling is bounded local IPC during an explicit operation, not
Cloudflare traffic. A successful wake setup may publish one changed device
configuration through the existing account path. No paid service or load test is
introduced. Evidence and rollback are outside Git in
`~/.local/share/beyondremote-migration/turn220/`.

## Compact setup follow-up (turn 221)

The generated reference is `design/desktop-setup-reference.png`. The final native
Mac overview fits a small 800 × 600 window with collapsed status rows. A step
needing network setup opens automatically; ready sections start collapsed.
Local address/router/firewall facts populate without manual input. Two explicit
router rechecks completed, with server PIDs unchanged across the measured check.
This verifies router configuration, not reachability from a new outside network.

Four local router tests cover route safety, endpoint restriction, stable ports,
and disabled-owned versus foreign mappings. Three permission widget tests and
Dart analysis pass. Mac staging is installed and file/signature verified.
Windows installation is pending the explicitly requested repair of its existing
Mac-only SSH firewall rule; no expanded rule or Input Monitoring grant has been
applied without the pending confirmation. New ad-hoc builds still require actual
OS permission read-back. Keep those native acceptance gates open.

## Permission completeness follow-up (turn 222)

- Mac screen capture, Accessibility/device control and Input Monitoring are independently actionable. Outgoing-only clients retain the keyboard step without asking for incoming capture/control. Returning to the app rechecks OS state; denial, cancellation and revocation stay pending.
- Windows system installation is the only prerequisite for its service step. Readiness now checks the actual service as well as the saved stop option. Just-for-me remains an explicitly limited, no-machine-service installation.
- Mac builds declare Local Network usage and the network section opens its exact privacy pane. A router timeout is not called a confirmed permission denial; Check again repeats the bounded discovery.
- Microphone, files/folders and clipboard remain feature-scoped. Do not require Full Disk Access, disable endpoint protection, grant router admin access automatically, or claim a permission is ready just because Settings opened. Wake still requires supported hardware/driver and a physical sleep test; router/firewall setup still requires an outside-network test.
- Five permission widget tests pass, including independent keyboard setup and dependent Windows service setup. Analyzer has no warnings/errors (three pre-existing deprecation infos). Mac Rust and Flutter release builds pass; all 175 installed staging file hashes and the signature verify. Native UI confirms the Input Monitoring action is enabled before screen capture permission. Tyler chose to grant it himself through the wizard.
- Approved Windows SSH rule remote address changed to 192.168.5.97 only; TCP 2222, Private profile, local binding and executable remain constrained. Windows-local TCP test succeeds, but Mac SSH still times out. Windows build/install acceptance remains open.

Apple reference: https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy
Evidence: `~/.local/share/beyondremote-migration/turn222/`. These checks are local OS calls: zero additional Cloudflare requests.

## Persistent Mac identity (turn 223)

Staging packaging now requires a saved Apple signing identity and refuses an ad-hoc fallback. Configure its SHA-1 fingerprint with `--identity`, `BEYONDREMOTE_SIGNING_IDENTITY`, or the user-local `~/.config/beyond-studios/beyondremote-signing.json` (`staging_identity`). This file contains an identity reference, not the private key; the key stays in Keychain. Keep the same staging bundle ID and signing identity for subsequent updates. Public distribution still needs Developer ID/notarization; the local Apple Development signature is not a notarized public release.

Nested Mach-O helpers and frameworks are signed inside-out, then the application; deep/strict verification and the Apple certificate anchor must pass. Ordinary installs must quit the staging app, preserve a rollback copy, and replace the complete bundle rather than patch a running executable. Never transfer TCC database entries, silently reset permissions, use ad-hoc signing on a missing certificate, or borrow the stable app's bundle ID.

The keyboard check accepts Accessibility's existing listening authority. Normal Mac setup presents only Screen Recording and mouse/keyboard control. Input Monitoring is offered only for outgoing-only clients, disappears when access is covered, and returns if revoked; these clients do not have to grant incoming device control.

Acceptance requires two differently sealed builds with the same designated requirement, a real grant on the first signed build, a complete-bundle reinstall, and live OS read-back on the second. Matching identity alone is not permission-retention proof.

The first identity listed by Keychain was revoked despite `find-identity -v` accepting it locally. Gatekeeper blocked and trashed that candidate. Packaging now checks the exact certificate with `security verify-cert -p codeSign -R ocsp` before creating output; the revoked-identity negative test passes. The second saved identity passes that check, and its installed 175-file bundle launches normally. No Gatekeeper, revocation or quarantine protection was disabled. After toggling the old entries and restarting still left native checks denied, a one-time, app-scoped `tccutil reset Accessibility us.beyondstudios.beyondremote.staging` and `tccutil reset ScreenCapture us.beyondstudios.beyondremote.staging` cleared the stale approvals. The existing Screen Recording and device-control permissions were restored through System Settings for the correctly signed app; stable BeyondRemote permissions were preserved. Local Developer signing does not pass a public-distribution `spctl` assessment and must not be shipped as a notarized release.

Six permission widget tests, targeted analyzer (only three existing deprecation infos), Python syntax, diff checks and Mac Rust/Flutter release builds pass. Real permission-retention acceptance subsequently passed as recorded below. Windows installation remains pending; do not infer it from Mac build/signature success.

Windows remote desktop connects, but reports a foreground-elevation restriction. Tyler reports no Administrator window is open, so the underlying cause is unconfirmed; do not equate that protocol flag with a verified window. The Mac-only SSH path still times out. The update is not installed there yet: restore a verified control/build path, inspect/reconcile the existing source, then run `scripts/windows/rebuild-staging.ps1` against the exact reviewed commit. Do not weaken UAC or widen the SSH firewall rule.

### Reinstall acceptance

The simplified build first reported both Screen Recording and device control **Allowed** through the running app's native checks. It was quit, then all 175 bundle files were replaced with a separately signed build (CFBundleVersion 68 → 69) with a different code seal and an identical designated requirement. All installed file hashes and deep/strict signatures passed. On relaunch it immediately showed **Device access is set up**; opening Review confirmed both permissions still **Allowed**. No permission reset, toggle or new approval was performed between those two builds. This verifies local permission retention across the tested reinstall, not public notarization or every future macOS release.

Evidence: `~/.local/share/beyondremote-migration/turn223/reinstall-tested-signatures.json` and `trusted-reinstall-installed-manifest.json`. The current installed app uses the same final Flutter/Rust code as this branch; build number 69 distinguishes the reinstall rehearsal. The active Mac native setup has only two required steps. These checks add zero Cloudflare operations.
