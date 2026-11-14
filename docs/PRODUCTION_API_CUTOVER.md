# BeyondRemote production API cutover checklist

Status: **production API and database cut over to Cloudflare on November 13,
2026**. The checklist below records the procedure; current acceptance and the
completed DNS-bridge retirement are in [the cutover tracker](API_MIGRATION_227.md).
InMotion retains private rollback artifacts and other sites; it no longer forwards API traffic.
Do not cancel the host yet. A passing rehearsal alone is not release authorization.

## Destination preparation

1. Preserve the existing production GitHub and Google client IDs, secrets and
   `https://api.beyondstudios.us/auth/{provider}/callback` redirect URLs. Test
   staging with separate clients; never repoint the production OAuth application
   to staging. Preserve provider/subject identity, not an email-based merge.
2. Create a separate production D1 database; never bind production to staging.
   Set `ENVIRONMENT=production`, `DATABASE_ROLE=production`,
   `BASE_URL=https://api.beyondstudios.us`, `PRODUCTION_ENABLED=1` and
   `MIGRATION_READ_ONLY=1`. The production enable flag is separate from the
   read-only fence. Preserve production signup policy deliberately.
3. Pin the reviewed Worker bundle, config, schema, source snapshot digest and
   client packages. Verify all current server-owned custom code before replacing
   it. Retain strict SSH and the original DNS/API/OAuth configuration privately.

## Final data handoff

1. Pause legacy writes for the transfer window. A read-only export does not stop
   writes on the source; an earlier snapshot is not a final delta reconciliation.
2. Export all seven source tables in one repeatable-read transaction. Preserve
   user IDs, provider IDs, bcrypt hashes, token hashes, address books, peers,
   tags, devices, timestamps and intentional disabled states. No raw credential
   values or database rows belong in Git, chat or a public report.
3. Run `node rehearse-restore.mjs PRIVATE_SNAPSHOT PRIVATE_REPORT` from
   `sync-api/cloudflare`. This runs local workerd/D1 and deletes its disposable
   local database after checking every original column and recovery.
4. Convert PostgreSQL timestamp timezone suffix `+00` to `+00:00` (including
   the snapshot time). Preserve subsecond precision. SQLite `julianday` does not
   accept the original short offset; merely copying strings silently invalidates
   expiry checks. Normalize booleans to 0/1.
5. Import into the verified empty production destination in a transaction. Compare
   every original column, counts and foreign keys. Explicitly promote only valid
   source sessions for enabled users to `native=1` on production. Staging must
   continue refusing imported production sessions. Keep imported OAuth requests
   `native=0`; users with an interrupted sign-in restart that sign-in.
6. Reconcile source changes made after the snapshot or restart the fenced import.
   Never overwrite current staging sessions, device settings or user work.

## Acceptance and switch

- Verify both real OAuth providers, password compatibility, owner-only data,
  disabled-account denial, logout, one-use connection/wake grants and role fences.
- The owner separated API hosting migration from app promotion on November 13,
  turn227. Preserve the stable API contract and existing production client config
  with `ACCOUNT_SERVER_SELECTION=0`. Do not replace app packages or merge staging
  device settings into production merely to move the API. Test legacy request
  envelopes, existing-session continuity and both provider callbacks at cutover.
- Before a later **app** promotion, test Windows↔Mac capture/input, disconnect,
  focused shortcuts, local logout with an open session, real sleep/wake with an
  awake LAN helper, and the deferred physical iPhone flow. These remain app gates,
  not reasons to retain the PHP/PostgreSQL hosting dependency.
- Local logout/account changes close native account streams without hosted
  polling. A remote administrator deleting a database session denies subsequent
  API requests and handshakes; it does **not** push a termination into an already
  established stream. Do not claim immediate remote active-stream revocation
  without implementing and qualifying an explicit push protocol.
- Verify both providers and private endpoints through normal production DNS/TLS,
  then remove the read-only fence and enable the accepted destination. Confirm
  settings/identity parity and a bounded real connection. No Cloudflare streaming
  relay, paid build service or unbounded hosted load test is part of this release.

## Recovery

Before destination writes are enabled, restore the exact old DNS/API route and
unfence the unchanged PostgreSQL source. After destination writes are enabled,
first fence both sides and reconcile the new destination records back to the
source; blindly pointing DNS back would lose new logins/device edits. Preserve
the destination snapshot and evidence. Keep callback URLs stable throughout.

After accepted migration, add the previously requested once-daily, seven-day
performance/cost checks; do not start or extend the window while cutover is pending.

## DNS propagation and independent app recovery

Production now accepts only its canonical origin. Both workers.dev and preview aliases are off. Staging remains separate; the explicit production-preview mode retains its write fence and uses the canonical host. Do not deploy read-only mode over live traffic for routine testing.

During propagation the legacy PHP forwarded to the same D1 writer; the forwarding bridge is now retired. The retained PHP backup can restore forwarding only in conjunction with the prior Worker transport and only after specifically assessing the need. Never restore the older PHP database writer or redirect production to the stale database. See the tracker for the exact version, evidence, and remaining sites.

Windows recovery is explicitly deferred until Tyler can approve locally. Inspect
`C:\Users\tyler\Documents\BeyondRemote-Staging-Handoff\install-keyboard-85f94fd23.ps1`
and its `keyboard-85f94fd23-install.json` receipt before rerunning anything. Verify
the 96-file package manifest, prior 98-file installation backup, staging service,
TCP21128 listener and owned router mapping. Preserve stable files, accounts and
Mica styling. Approve the prepared installer locally if needed; do not blindly
rebuild or relax UAC/firewall rules. Then verify internet reconnect and physical
Windows focus/shortcut behavior. Mac Remote/Local shortcuts passed Tyler's
physical acceptance; Windows post-update reconnection remains unverified.
