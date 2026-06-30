param(
    [string]$EnvPath = ".\cpanel.env",
    [string]$PublicDir = "/home/flip2t5/api.beyondstudios.us",
    [string]$PrivateDir = "/home/flip2t5"
)

$ErrorActionPreference = "Stop"

function Read-EnvFile($Path) {
    $map = @{}
    Get-Content -LiteralPath $Path | ForEach-Object {
        if ($_ -match '^\s*#' -or $_ -notmatch '=') { return }
        $idx = $_.IndexOf('=')
        $map[$_.Substring(0, $idx)] = $_.Substring($idx + 1)
    }
    return $map
}

function PhpString($Value) {
    return "'" + ([string]$Value).Replace('\', '\\').Replace("'", "\'") + "'"
}

function EnvOrDefault($Name, $Default) {
    if ($envMap.ContainsKey($Name) -and -not [string]::IsNullOrWhiteSpace($envMap[$Name])) {
        return $envMap[$Name]
    }
    return $Default
}

function Save-CpanelFile($Dir, $File, $Content) {
    $body = @{ dir = $Dir; file = $File; content = $Content }
    $result = Invoke-RestMethod `
        -Uri "$script:CpanelHost/execute/Fileman/save_file_content" `
        -Headers $script:CpanelHeaders `
        -Method Post `
        -Body $body `
        -TimeoutSec 60
    if ($result.status -ne 1) {
        $errors = if ($result.errors) { $result.errors -join '; ' } else { 'unknown error' }
        throw "Failed to upload ${Dir}/${File}: $errors"
    }
    Write-Host "uploaded ${Dir}/${File}"
}

$envMap = Read-EnvFile $EnvPath
$script:CpanelHost = $envMap["CPANEL_HOST"].TrimEnd("/")
$script:CpanelHeaders = @{
    Authorization = "cpanel $($envMap["CPANEL_USER"]):$($envMap["CPANEL_TOKEN"])"
}

$config = @"
<?php
return [
    'api_base_url' => $(PhpString $envMap["PUBLIC_API_BASE_URL"]),
    'db' => [
        'dsn' => $(PhpString "pgsql:host=localhost;port=5432;dbname=$($envMap["SYNC_DB_NAME"])"),
        'user' => $(PhpString $envMap["SYNC_DB_USER"]),
        'password' => $(PhpString $envMap["SYNC_DB_PASSWORD"]),
    ],
    'app_secret' => $(PhpString $envMap["APP_SECRET"]),
    'strategy_modified_at' => $(EnvOrDefault "CLIENT_CONFIG_VERSION" 1),
    'client_config_options' => [
        'custom-rendezvous-server' => $(PhpString (EnvOrDefault "CLIENT_ID_SERVER" "")),
        'relay-server' => $(PhpString (EnvOrDefault "CLIENT_RELAY_SERVER" "")),
        'api-server' => $(PhpString (EnvOrDefault "CLIENT_API_SERVER" $envMap["PUBLIC_API_BASE_URL"])),
        'key' => $(PhpString (EnvOrDefault "CLIENT_SERVER_KEY" "")),
    ],
    'oauth' => [
        'github' => [
            'client_id' => $(PhpString $envMap["GITHUB_CLIENT_ID"]),
            'client_secret' => $(PhpString $envMap["GITHUB_CLIENT_SECRET"]),
        ],
        'google' => [
            'client_id' => $(PhpString $envMap["GOOGLE_CLIENT_ID"]),
            'client_secret' => $(PhpString $envMap["GOOGLE_CLIENT_SECRET"]),
        ],
    ],
];
"@

Save-CpanelFile $PrivateDir "beyondremote-sync-config.php" $config
Save-CpanelFile $PublicDir "index.php" (Get-Content -LiteralPath ".\sync-api\cpanel\index.php" -Raw)
Save-CpanelFile $PublicDir ".htaccess" (Get-Content -LiteralPath ".\sync-api\cpanel\.htaccess" -Raw)

Write-Host "Beyond Remote sync API deployed."
