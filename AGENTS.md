# RustDesk Guide

## Project Layout

### Directory Structure
* `src/` Rust app
* `src/server/` audio / clipboard / input / video / network
* `src/platform/` platform-specific code
* `src/ui/` legacy Sciter UI (deprecated)
* `flutter/` current UI
* `libs/hbb_common/` config / proto / shared utils
* `libs/scrap/` screen capture
* `libs/enigo/` input control
* `libs/clipboard/` clipboard
* `libs/hbb_common/src/config.rs` all options

### Key Components
- **Remote Desktop Protocol**: Custom protocol implemented in `src/rendezvous_mediator.rs` for communicating with rustdesk-server
- **Screen Capture**: Platform-specific screen capture in `libs/scrap/`
- **Input Handling**: Cross-platform input simulation in `libs/enigo/`
- **Audio/Video Services**: Real-time audio/video streaming in `src/server/`
- **File Transfer**: Secure file transfer implementation in `libs/hbb_common/`

### UI Architecture
- **Legacy UI**: Sciter-based (deprecated) - files in `src/ui/`
- **Modern UI**: Flutter-based - files in `flutter/`
  - Desktop: `flutter/lib/desktop/`
  - Mobile: `flutter/lib/mobile/`
  - Shared: `flutter/lib/common/` and `flutter/lib/models/`

## Rust Rules

* Avoid `unwrap()` / `expect()` in production code.
* Exceptions:

  * tests;
  * lock acquisition where failure means poisoning, not normal control flow.
* Otherwise prefer `Result` + `?` or explicit handling.
* Do not ignore errors silently.
* Avoid unnecessary `.clone()`.
* Prefer borrowing when practical.
* Do not add dependencies unless needed.
* Keep code simple and idiomatic.

## Tokio Rules

* Assume a Tokio runtime already exists.
* Never create nested runtimes.
* Never call `Runtime::block_on()` inside Tokio / async code.
* Do not hide runtime creation inside helpers or libraries.
* Do not hold locks across `.await`.
* Prefer `.await`, `tokio::spawn`, channels.
* Use `spawn_blocking` or dedicated threads for blocking work.
* Do not use `std::thread::sleep()` in async code.

## Editing Hygiene

* Change only what is required.
* Prefer the smallest valid diff.
* Do not refactor unrelated code.
* Do not make formatting-only changes.
* Keep naming/style consistent with nearby code.

## Validation Rules

* Validate locally before pushing whenever practical.
* For Flutter/Dart edits, run `dart format --set-exit-if-changed` on changed
  Dart files, then `flutter analyze` on the same changed files.
* For Rust edits, run `rustfmt --check` on changed Rust files. Prefer a targeted
  `cargo check` for the edited crate or feature set when local toolchains allow.
  On this machine, use `rustfmt --edition 2021 --check <changed Rust files>`
  for file-level checks because bare `rustfmt` defaults to Rust 2015.
* Always run `git diff --check` before committing.
* Treat analyzer warnings and errors as actionable. Existing info-level lint
  noise may be reported without blocking if unrelated to the change.
* If a local platform toolchain is missing, state exactly which validation could
  not run and why.

## Toolchain Notes

* Normal release CI uses Flutter 3.24.5 / Dart 3.5.4 and Rust 1.75.
* `Cargo.toml` declares `rust-version = "1.75"`.
* Windows arm64 CI uses a patched newer Flutter path. Do not commit source
  changes that require newer Flutter APIs unless the workflow is being updated
  for all relevant platforms at the same time.
* Unsigned macOS workflow artifacts are ad-hoc signed and can be rejected by
  `spctl`; for local installs, copy the app bundle with `ditto` and clear
  `com.apple.quarantine` after download.
* iOS tag builds using `flutter build ipa --release --no-codesign` may only
  publish `liblibrustdesk.a`; install locally by building unsigned, embedding
  the Xcode-managed `com.tylerwalker.beyondremote` profile, then signing the
  app bundle for device install.
* If CocoaPods fails with `uninitialized constant ... Logger` on macOS, run
  Flutter/CocoaPods commands with `RUBYOPT=-rlogger`.
* For this machine's local macOS install build, use:
  `RUBYOPT=-rlogger VCPKG_ROOT=/Users/tylerwalker/vcpkg ./build.py --flutter --unix-file-copy-paste`.
  Skip `--hwcodec` locally unless its extra build environment is configured;
  CI can still build workflow artifacts with `--hwcodec`.
* Local macOS builds can rewrite only `flutter/macos/Podfile.lock` plugin
  checksums. Treat that as generated CocoaPods churn and do not commit it
  unless dependencies actually changed.

## Local Windows Install Notes

* For elevated installer scripts, do not block indefinitely on
  `Start-Process -Verb RunAs -Wait`. Start the elevated script, poll its
  transcript/log plus installed file hashes, and report success from those
  checks.
* For repeat elevated installs, prefer the native registered updater:
  `scripts/windows/register-elevated-installer.ps1`,
  `scripts/macos/register-elevated-installer.sh`, or
  `scripts/linux/register-elevated-installer.sh`. Trigger by writing the bundle
  path or update file path to the platform pending-update file, then poll the
  platform elevated-update log plus installed file hashes. Pending/log roots:
  `%PROGRAMDATA%\BeyondRemote`, `/Library/Application Support/BeyondRemote`,
  `/var/lib/beyondremote`.

## Localization (`src/lang/*.rs`)

Each file is a `HashMap<key, translation>`. Layout:

* `template.rs` is the master list of every key. **Never edit it** as part of translation work.
* `en.rs` holds only the keys whose English display text differs from the key itself.
* Every other file (`de.rs`, `fr.rs`, …) carries the full key set; an untranslated entry has an empty value: `("key", "")`.

### Finding the English source for a key

When filling an empty entry, determine the source English text with this rule:

* If `key` exists in `en.rs` **with a non-empty value**, that value is the source text (look it up in `en.rs`).
* Otherwise the **key string itself is the source text** (the key is already plain English).

Then translate that source into the file's target language (infer the language from the file's existing non-empty entries / filename).

### Translation hygiene

* Only fill empty values. Never change keys, and never touch existing non-empty translations.
* Preserve placeholders (`{}`) and escape sequences (`\n`, `\"`) exactly as in the source.
* Do not translate brand or technical tokens: `RustDesk`, `Socks5`, `TLS`, `UAC`, `Wayland`, `X11`, `TCP`, `UDP`, `2FA`, `RDP`, `D3D`, etc.
* Copy URL values (e.g. `doc_*` keys) verbatim from `en.rs`.
