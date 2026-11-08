param([Parameter(Mandatory)][string]$Installer,[Parameter(Mandatory)][string]$Payload)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path $Installer).Path
$payload = (Resolve-Path $Payload).Path
$dir = Join-Path $env:RUNNER_TEMP 'cd-install'
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudDesk'
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Test-Path $key) { throw 'key exists before install' }
& $exe install --silent --skip-checks --dir (Join-Path $env:RUNNER_TEMP 'missing') --payload (Join-Path $env:RUNNER_TEMP 'nope')
if ($LASTEXITCODE -eq 0) { throw 'install with bad payload must fail' }
& $exe install --silent --skip-checks --autostart --dir $dir --payload $payload
if ($LASTEXITCODE -ne 0) { throw 'install failed' }
foreach ($f in 'clouddesk.exe', 'clouddesk-widgets.exe', 'clouddesk-settings.exe', 'uninstall.exe', '.clouddesk-install') {
  if (-not (Test-Path "$dir\$f")) { throw "missing $f" }
}
if (-not (Test-Path $key)) { throw 'registry entry missing' }
if (-not (Get-ItemProperty -Path $run -Name CloudDesk -ErrorAction SilentlyContinue)) { throw 'autostart missing' }
& $exe update --silent --skip-checks --dir $dir --payload $payload
if ($LASTEXITCODE -ne 0) { throw 'update failed' }
if (-not (Test-Path "$dir.bak\.clouddesk-install")) { throw 'backup missing after update' }
& "$dir\uninstall.exe" uninstall --silent --purge
if ($LASTEXITCODE -ne 0) { throw 'uninstall failed' }
for ($i = 0; $i -lt 40 -and ((Test-Path $dir) -or (Test-Path $key)); $i++) { Start-Sleep -Milliseconds 500 }
if (Test-Path $dir) { throw 'install dir not removed' }
if (Test-Path "$dir.bak") { throw 'backup not removed' }
if (Test-Path $key) { throw 'registry entry not removed' }
if (Get-ItemProperty -Path $run -Name CloudDesk -ErrorAction SilentlyContinue) { throw 'autostart not removed' }
