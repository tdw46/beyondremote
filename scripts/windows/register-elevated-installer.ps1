$ErrorActionPreference = 'Stop'

$TaskName = 'BeyondRemote Elevated Installer'
$Root = Join-Path $env:ProgramData 'BeyondRemote'
$Runner = Join-Path $Root 'elevated-update.ps1'
$Pending = Join-Path $Root 'pending-update.txt'
$SourceRunner = Join-Path $PSScriptRoot 'elevated-update.ps1'

New-Item -ItemType Directory -Path $Root -Force | Out-Null
Copy-Item -LiteralPath $SourceRunner -Destination $Runner -Force
if (!(Test-Path -LiteralPath $Pending)) {
    New-Item -ItemType File -Path $Pending -Force | Out-Null
}

$usersRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    'Users', 'Modify', 'Allow'
)
$pendingAcl = Get-Acl -LiteralPath $Pending
$pendingAcl.SetAccessRule($usersRule)
Set-Acl -LiteralPath $Pending -AclObject $pendingAcl

$action = New-ScheduledTaskAction `
    -Execute 'powershell.exe' `
    -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$Runner`""
$principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 10)

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $action `
    -Principal $principal `
    -Settings $settings `
    -Force | Out-Null

"Registered scheduled task: $TaskName"
