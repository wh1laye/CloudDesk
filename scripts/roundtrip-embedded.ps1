param([Parameter(Mandatory)][string]$Installer)
$ErrorActionPreference = 'Stop'
$alone = Join-Path $env:RUNNER_TEMP 'cd-alone'
New-Item -ItemType Directory -Force $alone | Out-Null
$exe = Join-Path $alone 'setup.exe'
Copy-Item (Resolve-Path $Installer).Path $exe
$dir = Join-Path $env:RUNNER_TEMP 'cd-install-embedded'
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudDesk'
if (Test-Path $key) { throw 'key exists before install' }
& $exe install --silent --skip-checks --dir $dir
if ($LASTEXITCODE -ne 0) { throw 'embedded install failed' }
foreach ($f in 'clouddesk.exe', 'clouddesk-widgets.exe', 'clouddesk-settings.exe', 'clouddesk-launcher.exe', 'uninstall.exe', '.clouddesk-install') {
  if (-not (Test-Path "$dir\$f")) { throw "missing $f" }
}
if (Get-ChildItem $env:TEMP -Directory -Filter 'clouddesk-payload-*' -ErrorAction SilentlyContinue) { throw 'staging dir left behind' }
& "$dir\uninstall.exe" uninstall --silent --purge
if ($LASTEXITCODE -ne 0) { throw 'embedded uninstall failed' }
for ($i = 0; $i -lt 40 -and ((Test-Path $dir) -or (Test-Path $key)); $i++) { Start-Sleep -Milliseconds 500 }
if (Test-Path $dir) { throw 'install dir not removed' }
if (Test-Path $key) { throw 'registry entry not removed' }
