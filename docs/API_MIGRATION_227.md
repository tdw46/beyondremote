# BeyondRemote hosting cutover — November 13, 2026

Owner direction: finish the InMotion hosting exit independently of deferred app
quality-of-life work. Do not attempt another Windows elevation or app replacement
until Tyler is available locally. The recovery handoff is in
[the cutover checklist](PRODUCTION_API_CUTOVER.md#dns-propagation-and-independent-app-recovery).

## Verified preparation

- Live PHP SHA256 remains `0ca083cc4ef8a5db3a82cbd7b2a5f9a61af8d9435f4e6541ad83b814f1c6bb11`;
  `.htaccess` remains `7f297c6b3aad8f629b51bac9a17645f4360782ba6aa255ae5e25765e75105bb1`.
- Current consistent snapshot: 2 users, 12 sessions, 19 OAuth requests,
  0 legacy address books, 4 personal peers, 2 tags and 4 devices: **43 records**.
  One session is unexpired. Local D1 restore, all original columns, foreign keys,
  session expiry/promotion and recovery parity pass.
- Separate production D1 `6cbb7240-6f58-413f-9608-796f96b21e3a` restores those
  records exactly: **163,840 bytes**. Initial import/check used 4 query requests,
  305 rows read and 188 written including indexes. Staging remains separate.
- Read-only preview deployment `fe77d7d6-a8f2-4520-8057-989ae76dbc4a` preserves
  production OAuth configuration privately and canonical `api.beyondstudios.us`
  callbacks. Production retains legacy server-selection behavior.
- 19 local checks pass, including built-workerd authentication, 100 concurrent
  synthetic accounts, owner isolation, logout/disabled users, one-use grants,
  ordered presence, write fences and exact-origin transport restrictions.
- Eleven hosted checks verify both owners' private reads, guest denial and
  blocked writes. Two five-minute test sessions were removed afterward.
- The stable Mac app's **existing production token** authenticates on the restored
  destination. User, address-book and device responses preserve legacy fields;
  empty address-book timestamps are generated per request on both versions.
  Three samples: legacy 428/405/387ms; native 145/198/212ms. These small samples
  include network variance and do not prove peak capacity.
- PHP bridge rehearsed on InMotion without installation: health200, private401,
  write-fence503. It retains Apache's Authorization fallback, verifies TLS and
  never opens PostgreSQL. It is only a DNS-propagation bridge.
- All **44 non-SOA/NS DNS records** copied and verified on Cloudflare's free zone,
  initially DNS-only with unchanged content/TTL/priority. Purelymail records remain
  intact. Registrar accepted `fred.ns.cloudflare.com` and `olivia.ns.cloudflare.com`;
  Cloudflare zone is active. Recursive DNS can retain old delegation temporarily.

## Cost evidence

Shared-account aggregates, November1 through November13 20:52UTC:
89,757 Worker requests, 350,606.659 CPU-ms, 0 reported Worker errors;
6,052,794 D1 rows read, 823,148 written. Six D1 databases total 171,491,328 bytes.
Delayed/adaptive aggregates are not an invoice or a per-visitor measurement.
Existing Workers Paid remains unchanged; no new paid service, hosted load test,
R2 copy or streaming relay. Current shared monthly inclusions: 10M Worker requests,
30M CPU-ms, 25B D1 reads, 50M writes, 5GB D1 storage. Sources checked today:
[Workers](https://developers.cloudflare.com/workers/platform/pricing/),
[D1](https://developers.cloudflare.com/d1/platform/pricing/).

New clients use event-driven sync; old versions may retain heartbeat requests.
Unchanged registration has zero measured database writes, but requests/reads still
count. Track compatibility traffic during the seven-day follow-up. Aggregate
monitoring is retained; billable request logs containing OAuth URLs are not enabled.

## Cutover gate

Preparation is not live migration. Require active TLS, fresh source/config/DNS
checks, a brief source write fence, final consistent snapshot/import and exact
parity, then normal-hostname authorization/identity acceptance. Preserve private
rollback source/config. After destination writes begin, reconcile them before any
rollback; restoring stale PostgreSQL or changing DNS back alone loses data.

Private evidence is in `~/.local/share/beyondremote-migration/turn227/`, outside Git.
No credentials or raw account/device records are included here.

## Other sites after BeyondRemote

Continue the per-site inventory: CommsOpen commerce/community; Tyler's website
and its private finance/study/content applications; Beyond Studios' root website;
Flip2Tasty's legacy blog/StellarGallery/StellarVacationEscape surfaces. Preserve
data and features: directory listings or failing pages are not retirement approval.
HAB and mail retain their separate monitoring. Do not cancel InMotion while other
sites or the propagation bridge still depend on it.
