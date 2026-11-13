# Bundled, fixed wake operation. No adapter restart, firewall change or reboot.
$ErrorActionPreference = 'Stop'
try { $adapters = @(Get-NetAdapter -Physical | Where-Object Status -eq 'Up' | Select-Object -First 8) } catch { exit 16 }
if ($adapters.Count -eq 0) { exit 10 }
$supported = 0; $configured = 0; $armed = 0; $driverErrors = 0
foreach ($adapter in $adapters) {
    try {
        $power = Get-NetAdapterPowerManagement -Name $adapter.Name -ErrorAction Stop
        if ([string]$power.WakeOnMagicPacket -eq 'Unsupported') { continue }
        $supported++
        Set-NetAdapterPowerManagement -Name $adapter.Name -WakeOnMagicPacket Enabled -NoRestart -ErrorAction Stop
        $readback = Get-NetAdapterPowerManagement -Name $adapter.Name -ErrorAction Stop
        if ([string]$readback.WakeOnMagicPacket -ne 'Enabled') { continue }
        $configured++
        & powercfg.exe /deviceenablewake $adapter.InterfaceDescription | Out-Null
        if ($LASTEXITCODE -ne 0) { continue }
        $wakeArmed = @(& powercfg.exe /devicequery wake_armed)
        if ($LASTEXITCODE -eq 0 -and $wakeArmed.Trim() -contains $adapter.InterfaceDescription) { $armed++ }
    } catch { $driverErrors++ }
}
if ($armed -gt 0) { exit 0 }
if ($supported -eq 0 -and $driverErrors -gt 0) { exit 16 }
if ($supported -eq 0) { exit 11 }
if ($configured -eq 0) { exit 12 }
exit 13
