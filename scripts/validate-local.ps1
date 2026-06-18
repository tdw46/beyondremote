param(
    [switch]$SkipRust,
    [switch]$SkipFlutter,
    [switch]$Release,
    [switch]$NoFlutterBuild,
    [string]$CargoFeatures = "flutter"
)

$ErrorActionPreference = "Stop"

function Invoke-Check {
    param(
        [string]$Name,
        [scriptblock]$Command
    )

    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found in PATH."
    }
}

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

Invoke-Check "Git whitespace check" {
    git diff --check
}

if (Get-Command rg -ErrorAction SilentlyContinue) {
    Invoke-Check "Paywall/pro wording guard" {
        rg -n "\b(Server Pro|RustDesk Server Pro|rustdesk-server-pro|pricing|paywall|subscription plan|licensedDevices|licensed_devices|license is activated|confirm the license|is_pro\(|static ref PRO|hbbs pro|Use RustDesk Pro|Nâng cấp lên Pro)\b" README.md docs src flutter\lib --glob "!src/lang/template.rs"
        if ($LASTEXITCODE -eq 1) {
            $global:LASTEXITCODE = 0
        } elseif ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        } else {
            throw "Paywall/pro wording guard found matches."
        }
    }
} else {
    Write-Warning "Skipping wording guard because rg is not installed."
}

if (-not $SkipRust) {
    Require-Command cargo

    Invoke-Check "Rust formatting" {
        cargo fmt --all -- --check
    }

    Invoke-Check "Rust check ($CargoFeatures)" {
        cargo check --locked --features $CargoFeatures --lib
    }

    if ($Release) {
        Invoke-Check "Rust release build ($CargoFeatures)" {
            cargo build --locked --features $CargoFeatures --lib --release
        }
    }
}

if (-not $SkipFlutter) {
    Require-Command flutter
    Require-Command dart

    Push-Location flutter
    try {
        Invoke-Check "Flutter dependencies" {
            flutter pub get
        }

        Invoke-Check "Flutter analyze" {
            flutter analyze
        }

        Invoke-Check "Flutter tests" {
            flutter test
        }

        if (-not $NoFlutterBuild) {
            $os = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
            $platform = if ($env:OS -eq "Windows_NT" -or $os -match "Windows") {
                "windows"
            } elseif ($os -match "Darwin|macOS") {
                "macos"
            } elseif ($os -match "Linux") {
                "linux"
            } else {
                ""
            }

            if ($platform) {
                Invoke-Check "Flutter $platform release build" {
                    flutter build $platform --release
                }
            } else {
                Write-Warning "Skipping Flutter desktop build for unsupported host platform."
            }
        }
    } finally {
        Pop-Location
    }
}
