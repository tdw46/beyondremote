# Run from the existing signed-in Windows staging workspace. Never targets stable.
param([Parameter(Mandatory=$true)][string]$ExpectedCommit)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$source = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$root = Split-Path $source
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$log = Join-Path $root "logs\setup-$stamp.log"
$status = Join-Path $root 'logs\setup-update-status.txt'
New-Item -ItemType Directory (Split-Path $log) -Force | Out-Null
try {
  Set-Location $source
  $head = (git rev-parse HEAD).Trim()
  if ($LASTEXITCODE -ne 0 -or $head -ne $ExpectedCommit) { throw 'Source revision mismatch' }
  # Preserve generated lock files, but do not build unreviewed source edits.
  $dirty = @(git status --porcelain --untracked-files=no)
  foreach ($line in $dirty) {
    if ($line.Substring(3) -notin @('flutter/pubspec.lock','flutter/windows/flutter/generated_plugin_registrant.cc','flutter/windows/flutter/generated_plugins.cmake')) {
      throw "Review existing work before building: $line"
    }
  }
  . (Join-Path $root 'staging-env.ps1')
  $env:BEYONDREMOTE_STAGING = '1'
  "Building $head" | Set-Content $status
  $ErrorActionPreference = 'Continue'
  & python build.py --portable --flutter --skip-portable-pack --hwcodec --vram *> $log
  $rc = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  if ($rc -ne 0) { throw "Build failed ($rc); see $log" }
  $bundle = Join-Path $source 'flutter\build\windows\x64\runner\Release'
  if (!(Test-Path (Join-Path $bundle 'rustdesk.exe'))) { throw 'Missing compiled executable' }
  $files = @(Get-ChildItem $bundle -File -Recurse | Get-FileHash -Algorithm SHA256)
  if ($files.Count -lt 10) { throw 'Incomplete staging bundle' }
  $files | Export-Csv (Join-Path $root "logs\setup-$stamp-sha256.csv") -NoTypeInformation
  $dest = Join-Path $env:LOCALAPPDATA 'Programs\BeyondRemote Staging'
  if (!(Test-Path (Join-Path $dest 'BeyondRemote Staging.exe'))) { throw 'Existing staging installation not found' }
  $owned = @(Get-Process | Where-Object { $_.Path -like ($dest+'\*') })
  foreach ($p in $owned) {
    $connections = @(Get-NetTCPConnection -OwningProcess $p.Id -State Established -ErrorAction SilentlyContinue | Where-Object { $_.RemotePort -ne 443 })
    if ($connections.Count) { throw 'Build ready. Disconnect staging sessions before installation.' }
  }
  $backup = Join-Path $root "artifacts\staging-before-setup-$stamp"
  Copy-Item -LiteralPath $dest -Destination $backup -Recurse
  foreach ($p in $owned) { Stop-Process -Id $p.Id; Wait-Process -Id $p.Id -ErrorAction SilentlyContinue }
  foreach ($f in $files) {
    if ((Get-FileHash $f.Path).Hash -ne $f.Hash) { throw 'Build changed after verification' }
    $rel = $f.Path.Substring($bundle.Length+1)
    if ($rel -eq 'rustdesk.exe') { $rel = 'BeyondRemote Staging.exe' }
    $target = Join-Path $dest $rel
    New-Item -ItemType Directory (Split-Path $target) -Force | Out-Null
    Copy-Item -LiteralPath $f.Path -Destination $target -Force
  }
  foreach ($f in $files) {
    $rel = $f.Path.Substring($bundle.Length+1)
    if ($rel -eq 'rustdesk.exe') { $rel = 'BeyondRemote Staging.exe' }
    if ((Get-FileHash (Join-Path $dest $rel)).Hash -ne $f.Hash) { throw 'Installed hash mismatch; backup retained' }
  }
  $p = Start-Process (Join-Path $dest 'BeyondRemote Staging.exe') -WorkingDirectory $dest -PassThru
  "Installed $head; verified $($files.Count) files; PID $($p.Id); backup $backup" | Set-Content $status
} catch {
  "FAILED: $($_.Exception.Message)" | Set-Content $status
  throw
}
