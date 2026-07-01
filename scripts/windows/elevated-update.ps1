$ErrorActionPreference = 'Stop'

$Root = Join-Path $env:ProgramData 'BeyondRemote'
$Pending = Join-Path $Root 'pending-update.txt'
$Log = Join-Path $Root 'elevated-update.log'
$InstallDir = Join-Path $env:ProgramFiles 'BeyondRemote'
$InstallExe = Join-Path $InstallDir 'BeyondRemote.exe'

New-Item -ItemType Directory -Path $Root -Force | Out-Null
Start-Transcript -Path $Log -Force
try {
    if (!(Test-Path -LiteralPath $Pending)) {
        throw "Missing pending update marker: $Pending"
    }
    $UpdatePath = (Get-Content -LiteralPath $Pending -Raw).Trim()
    if (!$UpdatePath -or !(Test-Path -LiteralPath $UpdatePath)) {
        throw "Missing update path: $UpdatePath"
    }
    if ((Get-Item -LiteralPath $UpdatePath).PSIsContainer) {
        Stop-Service -Name BeyondRemote -ErrorAction SilentlyContinue
        Get-Process BeyondRemote,rustdesk -ErrorAction SilentlyContinue |
            Stop-Process -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        robocopy $UpdatePath $InstallDir /MIR /R:3 /W:1 /NFL /NDL /NP
        if ($LASTEXITCODE -gt 7) {
            throw "robocopy failed with exit code $LASTEXITCODE"
        }
        Copy-Item `
            -LiteralPath (Join-Path $InstallDir 'rustdesk.exe') `
            -Destination $InstallExe `
            -Force
        sc.exe config BeyondRemote start= auto | Out-Null
        sc.exe failure BeyondRemote reset= 60 actions= restart/5000/restart/10000/restart/30000 | Out-Null
        sc.exe failureflag BeyondRemote 1 | Out-Null
        Start-Service -Name BeyondRemote -ErrorAction SilentlyContinue
    } elseif ([IO.Path]::GetExtension($UpdatePath) -ieq '.exe') {
        Start-Process -FilePath $UpdatePath -ArgumentList '--update' -Wait
    } else {
        throw "Pending update is not an exe or directory: $UpdatePath"
    }
    Clear-Content -LiteralPath $Pending
    if (Test-Path -LiteralPath $InstallExe) {
        Start-Process -FilePath $InstallExe -WorkingDirectory $InstallDir
    }
    'BeyondRemote elevated update complete.'
} finally {
    Stop-Transcript
}
