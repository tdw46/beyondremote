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
