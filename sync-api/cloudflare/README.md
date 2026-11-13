# BeyondRemote native staging API

Workers and D1 replacement for the PHP/PostgreSQL account API. Production remains
on InMotion until a separately approved final data/DNS cutover. This does not
replace the TCP/UDP rendezvous or relay services.

## Local verification

`npm ci`, `npm run build`, `npm test`. Tests use isolated local D1/workerd and
synthetic accounts; no hosted load test. The bundled-runtime OAuth test mocks
only official provider requests. Imported source records and secrets must stay
outside this repository. Password hashes retain PHP bcrypt compatibility.

`schema.sql` runs at deployment, never on a request. Import all original columns
with normalized full-row parity and foreign-key checks before acceptance.
Imported production session/auth-request records are retained but cannot
authenticate staging: only native destination sessions have `native = 1`.

Deployment uses the official cf CLI, the exact prebuilt output and a private
`--secrets-file`. Verify account, Worker and D1 IDs before mutations. Config is
staging-only, robots denies crawling, and API responses are no-store. Secrets:
`GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, optional Google provider credentials,
and `CLIENT_CONFIG_JSON`. Never copy production OAuth callbacks into staging.
Set only verified public client configuration in CLIENT_CONFIG_JSON; the API
server must remain this environment. Staging signup is intentionally disabled;
the existing signup contract is available behind ALLOW_SIGNUP with rate limits.

## Presence and cost

The updated client recognizes `sync_capabilities.presence = events-v1`. It checks existing
local transport state, sends startup/state/device changes, and sends no HTTP
heartbeat while unchanged. Requests carry a server-issued generation and
monotonic sequence. Replays cannot overwrite a newer device generation.
Retries stop after three attempts; authorization/conflict failures stop until
context changes. Presence is advisory (`online-reported`, `offline-reported`,
`unknown`); a crash or sleep may omit offline. Actual connect intent still uses
the existing rendezvous reachability checks. There is no hosted lease timer,
cron, queue, Durable Object or R2 dependency.

Authorization is checked on each private request. Lists are owner-scoped;
logout and disabled-account checks take effect immediately. Reads do not touch
session/device timestamps. Peer and tag mutations preserve atomicity. Auth
cleanup is bounded and login-triggered, affecting only expired native records.
OAuth polls back off to eight seconds and stop at the existing three-minute
client deadline; callback state is one-use, with up to three result deliveries
for network recovery during its ten-minute expiry.

Measured local fixture: unchanged currentUser: 6 statements / 8 rows read / 0
written; own peers: 2 / 6 / 0; open presence generation: 7 / 11 / 1; state
change: 3 / 7 / 1; duplicate state: 4 / 7 / 0. First device creation and index
maintenance add work. These counts are fixture evidence, not universal bounds.
100-account / 200-request concurrency testing stays local. Track the shared
account's Workers requests/CPU and D1 reads/writes/storage, not requests alone.

## Separate Mac staging app

Build with `BEYONDREMOTE_STAGING=1` and the normal local build command. Then run
`python3 scripts/macos/package-staging.py <built.app> '/Applications/BeyondRemote Staging.app'`.
The package has a separate bundle identifier, config/IPC namespace and URL scheme;
its default API is the staging Worker. It must not replace BeyondRemote.app,
register its own system service, or inherit production account tokens. Preserve
ID/relay/key preferences deliberately during setup; never clone a device's
identity and run both hosts concurrently. Local unsigned signing checks are not
proof of GUI launch or sign-in: verify those separately on the installed app.

## Account-only connection staging

The staging desktop shows only Sign in, the account's devices, and Connect.
Manual IDs, server/key configuration and password connection setup are absent.
Host permissions, 2FA, recording and display controls remain available. Direct
traversal and relay are internal transports, not separate user workflows.

Staging clients publish `account_connection: ticket-v1`; old devices are visible
but require an update before Connect is enabled. A connection uses an authenticated
`POST /api/v1/connect/grant` with source_id/target_id and then target-side
`POST /api/v1/connect/authorize`. Only the same account's registered devices are
eligible. The random ticket is hashed in D1, expires after 60 seconds, is consumed
atomically once, and requires the original native source session to remain valid.
No account bearer token is sent to the peer in the new staging protocol. Staging
hosts refuse password/click fallback after ticket denial. Existing stable builds
retain their legacy protocol until the coordinated migration is accepted.

A representative local ticket issue + authorization measured 2 Worker requests,
17 D1 rows read and 5 rows written including indexes, before later bounded cleanup.
At 10 connections/device/day, 1,000 devices imply 600,000 requests/month and at
least 1.5M row writes, plus cleanup, login, presence and all other sites. This is
an assumption, not a traffic forecast. Expired-ticket cleanup runs only on new
connection intent (max 100 rows per call); no background timer is added. The
account has Workers Paid already; zero additional service activation is not a
promise of zero future charges.

Current acceptance limits: the compiled rendezvous/relay endpoints time out from
both test computers. The Windows C++ setup currently fails catalog loading. Mac
GUI and local authorization tests do not establish cross-device playback, input,
NAT traversal, sleep/wake, or relay recovery. Track these separately before any
stable release or production cutover.

## Account server selection (turn 214)

Tyler's latest choice is **this Mac** (`tylers-macbook-pro`, device 236321073),
superseding the temporary laptop and earlier desktop choices. The owner can
change this preference without changing device identities or account access.

Authenticated `GET /api/v1/connection-servers` lists only the caller's devices.
`POST /api/v1/connection-servers/select` accepts `{"device_id": "..."}` and
requires an owned desktop. Re-selecting the same machine writes no rows. The
preference is stored separately from imported device reports and presence.
Unconfirmed or signed-out hosts do not expose a usable address/key in this API.
An active session and online report mean only `reported-ready`; actual internet
reachability must still be tested on connection intent. No timer or poll is added.

Selection-based routing uses `ACCOUNT_SERVER_SELECTION=1` in the turn215 release.
Do not activate until matching desktop clients preserve the authenticated LAN
path with an empty internet-server configuration and present server setup/status.
This compatibility gate keeps the current installed clients working. The old
September desktop report is not accepted as readiness by the new selection path.

Remaining acceptance: package/install the desktop server binaries on the Mac;
verify key identity, private permissions, lifecycle and listening ports; complete
account selection UI/client integration; verify direct traversal and relay from
a different network before claiming internet connectivity. The separate local
transport/signaling experiments are not included in this deployment. No TURN,
Durable Object, paid relay, router mapping or production cutover is activated.

## Bundled desktop server and setup wizard (turn 215)

The desktop account view includes a server selector and setup wizard. On the
selected desktop, setup uses bundled, pinned hbbs/hbbr 1.1.16 executables; desktop
build.py packages them for Windows/macOS/Linux. Mac server builds use the exact
upstream commit with Rust 1.98 independently of the app's Rust 1.75 toolchain;
Windows downloads the hash-pinned upstream archive. No runtime Python, hosted
build service, server download, or router password is required for automatic setup.

UPnP discovery is limited to the OS-reported local gateway, with bounded responses,
no HTTP redirects/proxy, and fixed TCP 21115–21117 / UDP 21116 targets. Existing
mappings belonging to another service are refused. Created mappings are read back
and removed on failure, normal stop or shutdown, after rechecking ownership.
Crash leftovers are recognized only for the same device/address/port/description.
The Mac routing-table fallback covers default-net's missing-gateway case.
Manual public-address setup is available when the owner has configured forwarding
through their router's own trusted UI. Router passwords are never handled by the
API. Automatic setup currently targets IPv4 WANIPConnection:1; CGNAT, double NAT,
guest isolation, unsupported router APIs, firewall permissions and public-IP changes
remain possible obstacles. A mapping is not proof of an internet desktop session.

Staging server data and keys use a separate owner-only directory. Both server
processes require the generated key. Existing account-owned device tickets and
private authorization remain required by the staging remote-control hosts. The
cloud API is not started on the desktop. Stopping the server pauses it until an
explicit restart/selection; ordinary refresh does not silently restart it.
The wizard samples local IPC only while setup is active (at most 90 seconds),
with no new idle HTTP heartbeat or router renewal timer. Refresh/reconnect applies
server selection to other clients; there is no claim of immediate push updates.

Validation: 16 isolated API tests; three router-module tests including actual
create/read-back/remove of all four mappings on the Mac's gateway; Rust 1.75
release builds on Mac and Windows. Both staging installations passed hash checks.
The Mac server is running with verified owned mappings, and Windows reaches its
TCP listeners on the LAN. Staging routing version is
`a401ee91-5428-48bc-aef6-4e205f4187b8`; cross-network desktop acceptance is pending.
See `docs/CONNECTION_SERVER_MIGRATION.md` in the repository root for runtime
evidence, default setup, cost boundaries and the requested mesh VPN evaluation.
Exploratory transport/signaling files from turn214 remain excluded from this release.
