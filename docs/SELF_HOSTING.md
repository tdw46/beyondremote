# Beyond Remote Self-hosting

Beyond Remote is intended to be useful with an open-source self-hosted server from the first run.

## Client Setup

Open `Settings -> Network -> Self-hosted ID/Relay server`.

On Windows and Linux, click `Install` in the managed self-hosted server panel. Beyond Remote downloads the open-source hbbs/hbbr release, starts both processes, reads the generated public key, and applies the local client configuration automatically.

On macOS, provide local `hbbs` and `hbbr` binary paths in the same panel. Beyond Remote can start and stop those binaries with the app, but upstream does not currently publish a macOS server zip.

You can also fill in values manually:

- `Self-hosted ID server`: the `hbbs` host, with a port if needed.
- `Self-hosted relay server`: the `hbbr` host, with a port if needed.
- `Self-hosted API server`: optional `http://` or `https://` endpoint for deployments that provide account or address-book APIs.
- `Key`: optional server public key.

Leave relay, API, or key blank when your deployment does not use them.

## Feature Behavior

Core remote desktop, file transfer, clipboard, relay, and direct connection behavior are client features and work with the normal open server flow.

Some account, shared address-book, policy, deployment, and web-console flows require server-side APIs. When a server does not expose a capability, the client reports that the configured server does not support it.
