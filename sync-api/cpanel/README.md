# Beyond Remote cPanel Sync API

This is a small PHP/PostgreSQL account and sync service for Beyond Remote.

It is designed for cPanel shared hosting:

- public document root: `/home/flip2t5/api.beyondstudios.us`
- private config: `/home/flip2t5/beyondremote-sync-config.php`
- public API base URL: `https://api.beyondstudios.us`

The app talks to this service for account login, OAuth callbacks, synced address
book data, and device lists. Secrets stay server-side in the private config file.

Expected OAuth callback URLs:

- GitHub: `https://api.beyondstudios.us/auth/github/callback`
- Google: `https://api.beyondstudios.us/auth/google/callback`

Deploy from the repo root:

```powershell
.\scripts\deploy_cpanel_sync_api.ps1
```

Notes:

- `cpanel.env` is intentionally ignored by git and supplies deploy secrets.
- InMotion currently serves this vhost with PHP 7.3, so keep the API compatible
  with PHP 7.3 even if cPanel labels the domain as PHP 8.x.
- ModSecurity can false-positive on API clients. It is disabled only for
  `api.beyondstudios.us`.
