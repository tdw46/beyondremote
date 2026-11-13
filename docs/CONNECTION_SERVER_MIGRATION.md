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

No mesh VPN is installed or shipped by the current server-setup release.

## Shared tracker gate

The BeyondStore tracker checkout's four shared refs were fetched. A fresh
read-only production manifest contains 876 eligible owned files; 51 differ from
that checkout, with none missing. Their deployment intent has not been reconciled
in this task. Per that repository's freshness gate, its source/tracker files were
left untouched. This owning-project record captures current progress without
overwriting concurrent CommsOpen work.
