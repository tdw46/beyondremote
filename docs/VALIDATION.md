# Local Validation

Use `scripts/validate-local.ps1` before pushing changes that touch Rust, Flutter, build scripts, or user-facing commercial-lock/self-hosting behavior.

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\validate-local.ps1
```

Useful focused runs:

```powershell
# Rust plus static guards only
powershell -ExecutionPolicy Bypass -File .\scripts\validate-local.ps1 -SkipFlutter

# Static guards and Flutter checks only
powershell -ExecutionPolicy Bypass -File .\scripts\validate-local.ps1 -SkipRust -NoFlutterBuild

# Rust release library build with the same Flutter feature used by CI
powershell -ExecutionPolicy Bypass -File .\scripts\validate-local.ps1 -SkipFlutter -Release -CargoFeatures flutter
```

The script checks Git whitespace, scans for commercial-lock wording regressions, runs Rust formatting and `cargo check`, and runs Flutter dependency/analyze/test/build steps when the corresponding toolchains are installed.
