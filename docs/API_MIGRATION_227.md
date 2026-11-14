# BeyondRemote hosting cutover — November 13, 2026

Owner direction: finish the InMotion hosting exit independently of deferred app
quality-of-life work. Do not attempt another Windows elevation or app replacement
until Tyler is available locally. The recovery handoff is in
[the cutover checklist](PRODUCTION_API_CUTOVER.md#dns-propagation-and-independent-app-recovery).

## Final status — November 13, 23:11 Eastern

**InMotion forwarding retired.** Production API traffic and account data are served by Cloudflare at `api.beyondstudios.us`. The original PostgreSQL database/configuration and a private, hash-verified copy of the forwarding bridge were retained for recovery. The installed PHP entrypoint now returns HTTP 410 without forwarding credentials or reading a database. No app reinstall, mail change or hosting cancellation was made.

Over seven hours elapsed after the cutover, beyond the combined four-hour delegation and fifteen-minute A-record TTLs. Parent delegation, the Mac, Cloudflare, Google and Quad9 resolved to Cloudflare. We checked the HTTP/HTTPS vhost logs and November archives, then verified current HTTPS log coverage using a distinctive TLS request pinned to the original address. Excluding that probe, all four post-cutover requests were non-API/non-OAuth 404s, three after the TTLs. No legitimate API clients were observed; this is evidence about this site's logs, not a guarantee about every possible cache.

Production Worker `9b52bcba-df7d-48de-9d2f-c6c337b7c1dd` inherited the original D1, canonical URL, rate limiter, provider credentials and client settings. It has no transport-origin exception. Both production aliases are disabled; the former workers.dev endpoint returned 404. The explicit production-preview mode retains its read-only fence on the canonical origin. Isolated staging is unchanged.

Before retirement, the original two accounts matched their snapshot fields; the Mac's existing session and both configured login providers passed. After retirement the same session successfully read its users, peers and address book; anonymous and invalid-token reads returned 401. Both real OAuth flows and logout revocation had already passed at cutover. All 19 isolated tests passed, including mocked provider callbacks, isolation, revocation, and alternate-host denial. No new OAuth grant, paid service, load test or owner logout was needed. Private redacted evidence: `~/.local/share/beyondremote-migration/retirement-20261113/`.

Next: keep the separate November 14–20 monitoring window, continue the remaining site migrations, and leave Windows staging recovery for its own local maintenance window. Do not reactivate the stale PostgreSQL writer.

## Historical production acceptance at cutover

**LIVE on Cloudflare since November 13, 2026, about 21:07 UTC.**
`api.beyondstudios.us` now uses Worker version
`2608210c-9132-453d-a42b-04cb5e1ecb06` and the separate production D1 below.
Normal-DNS HTTPS, the original saved Mac login, both real GitHub and Google
browser sign-ins, owner isolation, logout revocation and preserved legacy
connection-server configuration passed. Twenty-two bounded live probes took
86–286ms; these are network samples, not a capacity guarantee.

The final source snapshot at 21:06:04 UTC had SHA256
`b66e89c1a8631624977b99a798e7f6097bfc34ce9fa3b51b02f58e4344de05f8`.
All 43 records and original columns matched after the fenced refresh; foreign
keys and local recovery passed. The refresh/verification used 291 rows read,
179 written and 10 REST query requests. The subsequent real Google sign-in
created Tyler's separate Google identity; it is intentionally retained. Acceptance
sessions were revoked, exact test OAuth flow rows removed, and the existing
Mac session remains valid. Original source rows still match the final snapshot.

The source PHP no longer opens PostgreSQL: it is a temporary HTTPS forwarding
bridge for clients with cached InMotion DNS. Cloudflare is the sole writer.
All unrelated DNS/mail records were preserved. **Do not call the bridge fully
retired or cancel InMotion yet.** After delegation/record TTLs have expired,
verify normal DNS from independent resolvers and absence of old-client traffic,
then retire the bridge and production workers.dev transport alias together.
Source backups remain private outside the webroot. After native writes, any
rollback requires reconciling new D1 state; never restore stale PostgreSQL.

### Bridge retirement check — November 13, 16:29 Eastern

**Retained pending DNS cache expiry.** The Mac resolver, Cloudflare, Google and
Quad9 all return Cloudflare addresses, and the parent delegation names the two
Cloudflare nameservers. However, only about 23 minutes had elapsed since cutover.
The old zone's NS TTL was 14,400 seconds; its API A-record TTL was 900 seconds.
The old authoritative nameserver still returns InMotion. Successful recursive
samples do not prove every resolver has discarded its older delegation.

Eleven bounded normal-DNS HTTP checks passed (96–270ms): D1 health, both login
options, robots, guest/invalid-token denials and the original Mac session's
users/peers/address book. The two original account records retain all compared
identity, password-hash, role and status fields. That direct D1 comparison read
four rows and wrote zero; database allocation remains 163,840 bytes. The deployed
Worker and bridge hashes match the accepted cutover.

The accessible legacy logs contain no requests after cutover, but their newest
entry is 20:57:03 UTC, before the switch. This is **not proof of zero bridge
traffic**: freshness/flush coverage must be established before retirement.

At Tyler's request, the one-shot `beyondremote-dns-bridge-retirement` task is
scheduled for **November 13 at 20:30 America/New_York**: after the four-hour NS
window plus 15-minute record TTL and a small margin. It must repeat DNS, traffic,
source freshness and account/privacy checks, then retire the exact PHP bridge
and production transport alias together only if those gates pass. Preserve
rollback evidence, remove the obsolete production transport configuration, and
verify canonical access again. If a gate fails, keep the working bridge and
report it; do not automatically extend the task or restore stale PostgreSQL.
The independent November 14–20 daily monitoring schedule is unchanged.

Evidence: `~/.local/share/beyondremote-migration/turn229/retirement-check.json`.

Daily read-only checks are scheduled November 14–20 at 09:15 America/New_York,
seven runs only, under `beyondremote-post-migration-performance-and-cost-checks`.
Windows elevation/service-route recovery remains deferred; no app install or
remote-desktop streaming was moved through Cloudflare in this cutover.

## Verified preparation

- Before cutover, legacy PHP SHA256 was `0ca083cc4ef8a5db3a82cbd7b2a5f9a61af8d9435f4e6541ad83b814f1c6bb11`;
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

The cutover gate passed: active TLS, fresh source/config/DNS checks, a brief
source fence, final snapshot/import parity, and normal-hostname identity and
private-access acceptance. The server uses Python 3.6; the private cutover helper
was corrected to use explicit PIPE arguments before the source fence was set. Preserve private
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

Next-site assessment has started for `tylerdillonwalker.com`: public WordPress/
Salient pages and media, the Mandarin study app, and the owner-only finance app
with Google sign-in, Plaid and SimpleFIN must migrate together without losing
their distinct access rules. Read-only inventory measured 1,051,297,527 bytes
under uploads and 1,110,547,071 bytes under study, before deduplication/optimization;
these are source allocations, not proposed R2 usage. The private finance directory
has 17 files totaling 9,000,970 bytes, including three sessions; no private content
or credentials were exported. Thirty-six selected application/theme files match
the Mac, but live Git has 1,742 dirty entries and a different committed tip.
Reconcile that evidence before source changes or a staging build; do not commit
the live tree wholesale or push this site's deploy-on-push remote during planning.
Fresh strict-SSH fetch found nine live-only commits and zero Mac-only commits;
the dirty set is mostly Jetpack plus study audio/dictionary and other plugin
updates. Guest finance APIs retain 401/private-no-store behavior. The study app
has four PHP dictionary endpoints, so a static copy alone would lose features.
The source media inventory also includes study corpora/intermediate files that
need classification before publication; copying the whole tree is not a release.
Private assessment: `~/.local/share/beyond-studios/migrations/tylerdillonwalker/`.

Tyler authorized removal of the obsolete commissions page during this assessment.
Page 5879 was moved from published to WordPress Trash with a private page/metadata
recovery copy outside the webroot. No shared media or source files were removed.
Exclude it from migration publication; preserve current external commission links.

## Edge protection and scope

AI-training blocking is enabled for the Beyond Studios zone, with search and
user-directed agent policies preserved. An API-host-scoped custom rule blocks
identified unwanted harvesting/backlink crawler identities before the Worker.
Normal health200/private401 and SEOkicks403 probes passed. The production
workers.dev transport remains temporarily necessary for the propagation bridge;
it still requires authentication on every private route and is not protected by
the custom-domain zone firewall. Retire it with the bridge. Staging is separate.

The shared global AGENTS.md now requires cost-conscious Cloudflare edge crawler
enforcement on every migrated/new site. HAB's corresponding provider receipt is
`~/.local/share/hab-turn228/REPORT.md`; its canonical website and bot docs have
training/crawler enforcement, and public page burst protection is site-specific.
CommsOpen and other not-yet-migrated sites remain separate release work.
