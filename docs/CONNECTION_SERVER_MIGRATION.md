# Account connection setup and migration

## Target experience

The first desktop defaults to **Set up this computer as your server**. Other
devices use the account's selected desktop. Selection, device ownership and
one-time connection authorization stay account-scoped. Tyler's current selection
is the Mac; moving it to another desktop must be explicit.

The selected machine must be awake, online and running BeyondRemote. Its role is
connection coordination and fallback relay; an established direct session should
not send its desktop stream through Cloudflare.

## Turn 215 implementation and acceptance

- Client source: `791315d28` (includes `480a86fac`). Windows source was
  fast-forwarded from `0d2adc76e`; Mica history and existing generated lock changes
  were preserved.
- Bundled, pinned hbbs/hbbr 1.1.16 server packaging supports desktop builds.
  Rust 1.75 remains the application toolchain; the Mac server uses its separate
  Rust 1.98 build.
- Automatic router setup checks the OS gateway, creates only the four required
  mappings, refuses another service's mapping, verifies read-back, and removes
  owned mappings on normal stop/shutdown or setup failure.
- Real router rehearsal passed create/read-back/remove for TCP 21115, TCP 21116,
  UDP 21116 and TCP 21117. This is not external-network desktop acceptance.
- Sixteen isolated API tests passed, including cross-account denial and
  idempotent selection. Three router-module tests passed, including the actual
  bounded router rehearsal. Modified account UI/model analysis has no issues.
- The new Mac staging package is installed; executable/core/server hashes and
  deep signature verification pass. Stable Mac executable is unchanged and the
  prior staging bundle is retained privately. The installed UI displays the
  selected Mac, included server software, and uncompleted network checks.
- Windows Rust and Flutter builds passed; packaging uses the Windows native
  HTTPS trust store plus the pinned archive SHA-256. Packaging fixes are in
  `8803961fa` and `6753c3009`. The installed staging bundle passed file-hash
  checks and launched in the signed-in desktop session. Its pinned server
  executables and licence also passed manifest verification.
- Staging Worker `a401ee91-5428-48bc-aef6-4e205f4187b8` activates account-selected
  routing. The selected Mac automatically started both bundled servers. All four
  router mappings were read back with the expected owner and destination. The
  server/key directory is owner-only (0700). Windows reaches the Mac's discovery
  and relay TCP listeners on the LAN. The account sees the Mac reporting its
  server ready; Windows does not incorrectly start a second server.
- Three bounded hosted health/guest-denial checks passed. These are not an
  external-network or capacity test. Production API and DNS are unchanged.
- A different-network/hotspot desktop test remains pending Tyler's return.
  New Mac staging signing also requires checking Screen Recording permission
  before accepting inbound desktop capture; do not alter TCC outside normal OS UI.

Runtime evidence and private backups live in
`~/.local/share/beyondremote-migration/turn215/`. Credentials and private server
keys do not belong in this document or source control.

## Requests and cost boundaries

There is no added idle HTTP heartbeat. Opening/refreshing the server chooser
adds one authenticated Worker request and a bounded, owner-filtered device read.
Saving selection adds one authenticated mutation, one bounded device validation
and a conditional preference write (zero changed rows for the same selection).
The UI then reloads the chooser and existing account configuration. Setup progress
uses local IPC only, bounded to 90 seconds. UPnP exchanges stay on the LAN.

Direct desktop streams use endpoint bandwidth; fallback through the selected
computer uses that computer's bandwidth. No hosted relay, TURN, paid Actions,
Durable Object or optional billable service was enabled. These statements do not
promise unlimited provider usage or free third-party relaying. Hosted concurrency
load tests were not run; the 100-account fixture workload stays local.

## Mesh VPN evaluation requested by Tyler

Investigate an integrated, account-isolated mesh transport to improve connections
across different networks. Prefer direct encrypted peer connections and a
reachable fallback relay, with automatic path selection. Keep access limited to
BeyondRemote unless the user separately requests broader VPN/network access.
Do not route general browsing through the selected computer by default.

A VPN alone does not make every computer publicly reachable. Restrictive NAT,
blocked UDP, CGNAT, sleeping hosts and lost connectivity still matter. Any
supported desktop may be a remote-control target; being a reliable public relay
requires reachability and availability. An app-managed peer relay can use the
selected desktop when reachable, but universal fallback needs another reachable
endpoint. Do not silently purchase one or assume a consumer free tier permits
embedding into a multi-user product.

Protocol keepalives may be needed to maintain NAT mappings. Treat them separately
from application/API polling, measure bytes and battery impact, and avoid D1
writes per keepalive. Account isolation, device-key revocation and existing
connection authorization must continue even when the data path bypasses the API.

References reviewed:
- https://tailscale.com/docs/reference/connection-types
- https://tailscale.com/docs/features/peer-relay
- https://www.wireguard.com/quickstart/

Turn 216 implements the app-scoped private connection and wake flow below. It
reuses BeyondRemote's encrypted transport; it does not install WireGuard, a TUN
adapter, or a general-purpose IP mesh VPN.

## Turn 216: private network and Wake and connect

Implemented and installed on Mac and Windows staging on 2026-11-12. Desktop
source is `6e0a2e026`; the final API source is `4a0b718d2`. Staging Worker version
is `6306e637-6998-45e1-abe6-24f0feaf363f`. Production API/DNS and stable desktop
apps remain unchanged. Windows Mica styling and both platforms' existing
account/configuration data were preserved.

### Experience and transport

- Normal device cards retain **Connect**, with **Wake and connect** beside
  eligible computers. A collapsed **Private network & wake** group contains the
  home-computer choice, nested server setup, OS wake preparation and a per-device
  wake opt-in. The Mac remains the selected home computer.
- Connections try account-authorized private addresses and, for the selected
  running home server, its public address at fixed TCP 21128. Pinned device keys,
  encryption, ownership and one-use desktop authorization remain enforced.
  Existing traversal and selected-desktop relay fallback remain available.
- Automatic router setup now manages five owned mappings: TCP 21115, TCP 21116,
  UDP 21116, TCP 21117 and TCP 21128. All five passed real-router read-back.
  Shutdown removes owned mappings and restart recreates them. UDP 9 is never
  exposed to the internet.
- A wake request uses a separate, one-use 60-second account ticket. The awake
  selected home computer checks it over the existing encrypted control channel,
  revalidates permissions, then emits three fixed 102-byte LAN magic packets.
  It must have a physical adapter on the target's subnet. No arbitrary UDP
  forwarding or desktop access is granted by a wake ticket.
- Wake authorization is limited to three tickets per account per minute,
  including consumed tickets. Native receivers also bound authorization attempts.
  Revoked source sessions, removed devices, changed target permissions, stale
  helper sessions, cross-account requests and replay are rejected.
- OS preparation uses normal macOS authentication / Windows UAC. macOS enables
  AC wake for network access; Windows requests supported adapter wake settings
  without restarting adapters. Firmware support is not assumed. These privileged
  setup prompts still need an interactive acceptance test.

### Verification and honest remaining gates

- Seventeen isolated API tests pass, including twenty concurrent wake requests
  admitting exactly three, replay/revocation/cross-account denial, and the existing
  local 100-account workload. No hosted load test was run.
- Three Rust wake tests pass, including actual Mac physical-adapter discovery.
  A Darwin adapter-library MAC parsing issue was corrected using validated local
  interface discovery, cached until the interface/IP set changes.
- Changed Dart analysis is clean. Mac and Windows release builds pass; installed
  files match their manifests and Mac deep signature verification passes.
- Actual Mac-to-Windows desktop rendering and input passed. Enabling Windows
  wake in its UI appeared on the Mac after refresh. Clicking **Wake and connect**
  consumed its one-use ticket at `2026-11-13T02:49:52.370Z`; after the bounded
  eight-second wait it opened the Windows desktop directly at 02:50:02Z. The
  brief test was disconnected afterward. Windows was already awake: this proves
  the authorization/control/connection flow, not hardware wake from sleep.
- Both desktop staging apps have account wake enabled. The Mac reports AC wake
  enabled; Windows reports adapter magic-packet capability, but its armed-device
  list and Modern Standby behavior require a physical sleep/wake test. No machine
  was put to sleep remotely.
- **Pending:** different-network/hotspot connection, real sleep/wake, and normal
  Screen Recording permission for inbound Mac capture after staging re-signing.
  iOS wake UI and physical-device acceptance are not part of this installed
  desktop release. The helper must be awake and reachable on the target LAN;
  automatic helper failover is not implemented. This cannot guarantee access
  when all home machines sleep, CGNAT blocks reachability, or hardware lacks wake.

### Incremental operations and recovery

Wake adds zero idle hosted requests. Its explicit action adds two Worker requests
(prepare and consume); the isolated fixture measured 23 rows read and 5 rows
written. Later expiry cleanup is owner-indexed and bounded; budget approximately
four additional writes per grant including indexes. The following normal desktop
connection has its existing routing/authorization costs in addition to wake.
Local progress checking is bounded, and there is no hosted wake queue, polling,
R2 traffic, new paid service or hosted desktop relay.

| Wake volume | Added Worker requests/month | Fixture-based reads/month | Planned writes/month, including cleanup |
| --- | ---: | ---: | ---: |
| 1,000/month | 2,000 | 23,000 | approximately 9,000 |
| 100,000/month | 200,000 | 2,300,000 | approximately 900,000 |

These projections are assumptions, not account capacity measurements. At an even
100,000 wakes/month, about 30,000 writes/day would consume 30% of D1's free daily
write allowance before normal connections and other sites. Current shared free
allowances are 100,000 Worker requests/day, 5 million D1 rows read/day and 100,000
rows written/day, with 5 GB D1 storage. No new storage database was created and
no paid upgrade was enabled. Bursts and the account's other sites must be counted;
the next likely constraint for a wake-heavy workload is D1 writes.
Sources checked: [Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/)
and [D1 pricing](https://developers.cloudflare.com/d1/platform/pricing/).

Private evidence/manifests and the prior Mac staging bundle are retained under
`~/.local/share/beyondremote-migration/turn216/`. The Windows prior installed
bundle is under its handoff `artifacts/staging-before-wake-216`. Recovery can
restore those exact bundles and the previous Worker version; leave the additive
wake table in place rather than deleting account records. Disable wake through
its per-device switch to revoke further requests. Do not remove current server
keys, account data or router mappings owned by another service.

## Turn 217: real phone-hotspot test

On 2026-11-12, the Mac joined Tyler's iPhone hotspot while Windows remained on
home Wi-Fi. The default gateway changed from the home router to the phone, the
public IPv4 differed, and the Windows LAN SSH endpoint became unreachable. The
staging health API and authenticated account route both responded successfully
over cellular. This was a real separated-network test, not a LAN rehearsal.

**Result: internet connection failed.** After a fresh client launch, the native
session validated the account route and pinned key but could not reach either
Windows private address. Its fallback attempted the old home public endpoint on
TCP 21116 and timed out. The Mac was still the selected home server, but had left
that router. No replacement helper was selected or silently installed. This does
not establish that the opposite topology (Mac server remaining home, Windows on
cellular) fails; that topology remains untested.

Two additional recovery issues were exposed:

- Closing a remote window and connecting again can fail in the main UI before a
  new native session starts; restarting the staging app allowed a fresh attempt.
  The window lifecycle/error reporting needs correction.
- Returning home assigned the Mac a different DHCP address. The app refused its
  own stale mappings because their internal client still had the old address.
  All five entries were independently verified against the exact staging owner,
  old internal address and port, then removed. **Retry automatic setup** recreated
  all five for the new address; each was read back and both bundled server
  processes were running. A permanent network-change reconciliation fix is still
  required. Never overwrite mappings owned by another service.

Home Wi-Fi was restored. After restarting the Mac staging client, a new direct
Mac-to-Windows desktop session rendered successfully and was disconnected using
the in-session toolbar. Windows' remote-desktop listener remained reachable,
but its strict SSH runner no longer accepted the Mac's new source address; its
firewall/SSH restrictions were not broadened. The paired runner needs a verified
source-address update or restoration of its previous reserved address before
further SSH work. No host keys or credentials changed.

Next implementation: invalidate stale network/public-route claims on interface
changes, reconcile this device's owned mappings safely, recover closed remote
windows, and offer a clear home-server handoff when the chosen server leaves its
network. Re-test both network directions and keep actual sleep/wake separately
gated. No hosted relay, paid service or load test was enabled. Private recovery
evidence is in `~/.local/share/beyondremote-migration/turn217/`.

## Shared tracker gate

The BeyondStore tracker checkout's four shared refs were fetched. A fresh
read-only production manifest contains 876 eligible owned files; 51 differ from
that checkout, with none missing. Their deployment intent has not been reconciled
in this task. Per that repository's freshness gate, its source/tracker files were
left untouched. This owning-project record captures current progress without
overwriting concurrent CommsOpen work.


## Turn 218: direct internet connection through the phone hotspot

The failed turn-217 topology is now corrected. Each signed-in desktop can publish
its own verified direct router mapping; it no longer depends on the selected Mac
home server remaining at home. On 2026-11-12/13 the Mac joined the iPhone hotspot
and rendered the Windows laptop desktop across the public internet. Windows
remained on home Wi-Fi. Remote mouse input worked. Two initial cellular sessions
completed their account-route/direct-connect stage in approximately 1.4 seconds
each; these are individual observations, not a latency or capacity guarantee.

The live socket used the phone's private subnet and the home router's public
IPv4 on the device's assigned high TCP port. The Windows router mapping was read
back independently and matched its exact owner, internal address and listener.
Desktop video/input traveled directly between computers with the existing pinned
key and one-use account authorization. Cloudflare served account/control metadata
only; no video relay, hosted VPN, paid service, or OS-wide traffic tunnel was added.

### Fixes and protection

- Discover the current physical default route, bind discovery to that interface,
  and accept router control URLs only on the trusted local gateway.
- Give each desktop a stable high public TCP port mapped to its authenticated
  direct listener. Refuse foreign mappings. Reclaim a stale DHCP mapping only
  when its exact device owner, protocol and internal port match.
- Clear stale public-route claims on network changes, reconcile the selected
  local server, and bound discovery/retries locally. No new idle hosted polling.
- Publish public endpoints only for the signed-in account's ready device with a
  live linked session. Cross-account and revoked-session tests still deny access.
- Preserve the main window when pruning closed remote windows. Run shutdown
  cleanup on a fresh thread rather than after main-thread TLS destruction.
- Preserve connection/wake capability fields through Dart group serialization,
  peer copies, and the native encrypted cache reader; server checks remain
  authoritative even when the UI uses cached capability information.

### Operations and recovery

The isolated account-route fixture changed from two statements/six rows read to
three statements/eight rows read, with zero writes. Per 1,000 explicit route
lookups this adds about 2,000 rows read; per 100,000 lookups about 200,000. The
Worker request count for the lookup is unchanged. Existing grant, authorize and
wake costs remain additional. Router discovery and media streaming run on the
user's devices/network, not Cloudflare. Shared account limits still apply to
control traffic; no metered load test or new paid plan was enabled.

API permission tests (18), local router validation tests (3), and Dart cache
round-trip/copy tests (2) passed. Desktop builds preserve Windows Mica styling.
Private build/install manifests, socket/route evidence and rollback bundles are
under `~/.local/share/beyondremote-migration/turn218/` and the Windows handoff
logs/artifacts. Final installed-build acceptance is recorded below.

This proves Mac-on-cellular to awake Windows-at-home with this UPnP-capable home
router. It does not certify every router/CGNAT topology, an always-on iOS server,
or waking a physically sleeping machine. The existing authenticated wake flow
still requires a reachable awake helper on the target LAN. Inbound Mac screen
capture remains subject to macOS's Screen Recording permission.


### Final installed-build acceptance

Final native source `bb580964ef566a373747bc8dc603ca51400af47f` was built and
installed on both desktops. All 175 Mac bundle files matched the release
manifest, and deep/strict code-signature verification passed. The Windows
installer verified each source and destination hash before launching its final
build. Existing staging identities/settings and Windows Mica styling remained.

At 00:12:50 and 00:13:46 EST on November 13, the final Mac build connected from
the iPhone hotspot to Windows at home; native route-to-direct connection times
were 1.427 and 1.362 seconds. The Windows desktop rendered, remote mouse/scroll
input changed its UI, and both toolbar disconnect and native window close
returned to the device list. Mac PID 91660 stayed alive through both cellular
sessions and the subsequent return-home LAN session at 00:19:41. No manual device
refresh, app restart, VPN client, relay selection or address entry was needed
for these reconnections. Cache restoration was separately checked after a clean
quit/relaunch. No new Mac crash report appeared after the shutdown fix.

Home Wi-Fi and ordinary DHCP were restored (the temporary SSH-compatible manual
address was removed). The local bundled server processes recovered, and the
per-device public route was verified again after returning home. Phone hotspot
UPnP failure is expected and bounded; outbound connection to the home device
still works. The strict Windows SSH source restriction was left intact, so later
SSH development may need the separately authorized pairing/source-address work;
this does not affect the verified app connection.

The final session was disconnected, leaving BeyondRemote Staging ready on its
device list. Stable production app installations were not replaced. The shared
BeyondStore tracker gate above remains; this owning-project tracker and private
evidence are the current authoritative record for these connection fixes.


## Turn 219 readiness and installed logout verification — 2026-11-13

Production API/DNS remain on InMotion; migration is not yet finalized. The
reviewed cutover/rollback sequence is in `PRODUCTION_API_CUTOVER.md`.

### Completed

- A fresh repeatable-read production snapshot at 13:32:51 UTC contains 43 rows:
  2 users, 12 sessions, 19 OAuth requests, 0 legacy address books, 4 peers,
  2 tags and 4 devices. One source session is unexpired. Source entrypoint hash
  remains `0ca083cc4ef8a5db3a82cbd7b2a5f9a61af8d9435f4e6541ad83b814f1c6bb11`.
- The new local workerd/D1 rehearsal verifies every original column, foreign keys,
  deliberate production-only active-session promotion and recovery after a
  synthetic edit. It found and fixed PostgreSQL `+00` timestamps being unreadable
  to SQLite expiry checks; conversion to `+00:00` retains fractional precision.
  The one active source session remains active in the isolated rehearsal.
- Imported pending OAuth responses cannot be redeemed: native-only guards now
  cover start, callback and response claim. Built-workerd tests exercise Google
  and GitHub, provider mismatch and state replay. Environment/role mismatch and
  unapproved production activation fail closed. A read-only destination fence
  blocks writes and OAuth claims during acceptance.
- All 19 API tests pass, including 100 concurrent synthetic accounts/200 requests
  locally. No hosted load test or database import was performed.
- Staging Worker version `3ec23997-569a-45bd-b9f2-39122bba265a` is deployed.
  Existing secrets and the staging DB binding were preserved. Live health,
  GitHub provider listing, guest 401/no-store and robots checks pass. Bare urllib
  probes failed before repeating with an identified acceptance user agent;
  those failures are not counted as application acceptance.
- A local watch event closes inbound account streams after local logout/account
  replacement; unchanged credentials do not interrupt them. The UI closes outgoing
  staging session windows on account reset. No new HTTP timer or heartbeat.
  The cleared service credential cannot fall back to a stale local token after
  an account-change event. Rust check, targeted event test and Dart analysis pass.
- Mac release Rust and Flutter builds pass. Xcode 27 required a local staging
  deployment-target override of 12.0. Packaging preserved the modified runtime
  database in the pinned server checkout and reused the exact hash-verified,
  unchanged bundled hbbs/hbbr binaries. The installed staging bundle's 175 files
  match its manifest and deep/strict signing verifies. Stable app is unchanged.
- The updated installed Mac app connected to the Windows laptop and rendered its
  desktop. With that session open, Logout returned the main app to Login and the
  remote window disappeared from the Window menu. GitHub sign-in then restored
  the same account and device list. This is real outgoing-session logout proof,
  not inbound capture or remote-admin push-revocation proof.

### Remaining release gates

- Separate Google staging client form is prepared in the existing Beyond Remote
  Google project, with the exact staging callback. Creation/secret storage awaits
  the computer-use action-time credential approval. Production still has both
  Google and GitHub; staging currently advertises GitHub only.
- macOS System Settings explicitly shows BeyondRemote Staging Screen & System
  Audio Recording OFF, stable app ON. Enabling staging awaits action-time capture
  permission approval. Windows-to-Mac capture is not accepted yet.
- Strict SSH to the saved Windows runner address timed out. App connectivity did
  work; these are separate transports. Do not weaken host keys or firewall rules
  to hide the runner issue. Windows still needs this turn's logout build and the
  real sleep/wake test. Existing awake-helper wake proof is not sleeping-device
  proof. Physical iPhone acceptance remains deferred by Tyler.
- Stable production clients still use legacy behavior behind `STAGING_BUILD` and
  app-name gates. Prepare/accept the native production client promotion together
  with the final fresh data sync and API cutover; DNS alone is insufficient.
- Remote deletion/disablement denies new API access and handshakes, but there is
  no server-push revocation of an already established media stream. Local logout
  is fixed; do not equate it with immediate remote administrative revocation.

### Cost and records

The restore and concurrency runs used local workerd/D1: zero hosted DB rows
read/written. Runtime permission/event fixes add zero hosted requests. Production
activation guards run before DB access, and OAuth's native flag adds no query.
Existing route/grant/authorize operation counts remain applicable. No new paid
service, media relay, hosted load test or polling was enabled. Historical free
plan figures above are not a current account plan claim; budget against the
shared actual account plan before cutover, including other sites.

Private evidence, exact manifests, source snapshot and rollback bundle are under
`~/.local/share/beyondremote-migration/turn219/`. The shared BeyondStore tracker
remains blocked by its unresolved production/source freshness comparison; this
owning-project tracker records the current work without overwriting that branch.


## Turn 220 Google staging configuration and inbound capture — 2026-11-13

- Created the approved **BeyondRemote Staging** Google web OAuth client in the
  existing Beyond Remote project, with only the exact staging callback and no
  JavaScript origins. The production client remains unchanged. The generated
  credential is stored owner-only in the global Beyond Studios credential store,
  outside Git, and installed as staging Worker secrets. Read-back verifies both
  Google bindings and the preserved GitHub bindings; live login options now show
  both providers.
- After Tyler approved basic Google identity access, the real Google consent and
  callback completed token exchange/profile lookup. The API returned **Signup
  unavailable on this preview**, as expected: the source snapshot has password
  and GitHub accounts but no Google identity, and staging keeps `ALLOW_SIGNUP=0`.
  This verifies real provider wiring and new-account denial, not successful login
  to an existing Google account. No email-based account merge or signup-policy
  change was made. Existing-user Google behavior has the local bundled-worker
  coverage recorded in turn 219.
- System Settings now shows Screen & System Audio Recording **ON** for
  BeyondRemote Staging. It was already on at read-back, so no toggle was needed.
  At 09:08:18 EST the Mac log records an authorized same-account inbound session
  and successful capturer initialization. Tyler confirmed the Windows client
  displays the Mac desktop. This closes the inbound Mac capture gate; it does
  not claim new audio/input or sleep/wake acceptance.
- A fresh strict SSH attempt to the saved Windows runner still timed out. The
  working app connection is independently verified; runner transport restrictions
  remain intact. Windows still needs the turn 219 logout build and real sleep/wake
  acceptance. Production-client promotion, final fenced data synchronization and
  API/DNS cutover remain outstanding; production is not finalized.
- No new polling, hosted load test, streaming relay or paid service was enabled.
  Google testing used one bounded sign-in flow and aggregate configuration reads;
  this is not a new traffic/capacity or shared-account billing measurement.

Evidence: `~/.local/share/beyondremote-migration/turn220/` contains the safe client
list/capture screenshots, provider-binding read-back and redacted inbound event
record. Source and shared BeyondStore tracker restrictions from turn 219 remain.
