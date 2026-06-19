# Beyond Remote Self-hosting

Beyond Remote is intended to be useful with an open-source self-hosted server from the first run.

## Client Setup

Open `Settings -> Network -> Self-hosted ID/Relay server`.

On Windows and Linux, click `Install` in the managed self-hosted server panel. Beyond Remote downloads the open-source hbbs/hbbr release, starts both processes, reads the generated public key, and applies the local client configuration automatically.

On macOS, provide local `hbbs` and `hbbr` binary paths in the same panel. Beyond Remote can start and stop those binaries with the app, but upstream does not currently publish a macOS server zip.

## Public Access Options

Local-only setup uses `127.0.0.1` and is useful for testing on one computer. Other networks cannot reach that address.

Home-router setup uses your home public IP address or dynamic DNS name. Forward TCP ports `21115-21119` and UDP port `21116` from your router to the computer running the managed server. This is free if your internet provider allows inbound connections, but it exposes your home IP address and depends on your router/ISP.

Public VPS setup is the easiest internet-ready option. A VPS is a small always-on Linux machine with a public IP address. Run `hbbs` and `hbbr` there, point a DNS name at it if you want one, and use that host as the public address in Beyond Remote. The software and automation can be fully open-source; the VM itself must be provided by the user or by a cloud free tier. Oracle Cloud Always Free is a practical candidate when capacity is available. Paid low-cost VPS providers work too, but are not free.

Fully automatic per-user VPS setup is possible when the user provides either:

- SSH access to an existing VPS. Beyond Remote can install server binaries, create services, open firewall rules, start the server, read the public key, and apply the client settings.
- A cloud-provider API token. Beyond Remote can create the VM, configure it with cloud-init or SSH, then apply the same server settings. This can be implemented with open tooling such as shell scripts, cloud-init, or OpenTofu/Terraform-style templates, but each provider has its own API, terms, and free-tier limits.

Vercel-style web hosting is not a good target for the ID/relay server. Beyond Remote's relay needs always-on TCP and UDP ports, while web/serverless hosts are designed for HTTP requests and bounded function execution.

You can also fill in values manually:

- `Self-hosted ID server`: the `hbbs` host, with a port if needed.
- `Self-hosted relay server`: the `hbbr` host, with a port if needed.
- `Self-hosted API server`: optional `http://` or `https://` endpoint for deployments that provide account or address-book APIs.
- `Key`: optional server public key.

Leave relay, API, or key blank when your deployment does not use them.

## Feature Behavior

Core remote desktop, file transfer, clipboard, relay, and direct connection behavior are client features and work with the normal open server flow.

Some account, shared address-book, policy, deployment, and web-console flows require server-side APIs. When a server does not expose a capability, the client reports that the configured server does not support it.
