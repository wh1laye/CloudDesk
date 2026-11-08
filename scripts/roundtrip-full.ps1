param([Parameter(Mandatory)][string]$Installer,[Parameter(Mandatory)][string]$Payload)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path $Installer).Path
$payload = (Resolve-Path $Payload).Path
$dir = Join-Path $env:RUNNER_TEMP 'cd-install-full'
$wl = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon'
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\CloudDesk'
$backup = Join-Path $env:APPDATA 'CloudDesk\shell-backup.txt'
$before = (Get-ItemProperty -Path $wl -Name Shell -ErrorAction SilentlyContinue).Shell
$want = '"{0}\clouddesk-launcher.exe"' -f $dir
& $exe install --silent --skip-checks --profile full --dir $dir --payload $payload
if ($LASTEXITCODE -ne 0) { throw 'full install failed' }
foreach ($f in 'clouddesk-launcher.exe', 'restore.cmd') {
  if (-not (Test-Path "$dir\$f")) { throw "missing $f" }
}
if (-not (Test-Path $backup)) { throw 'shell backup missing' }
$now = (Get-ItemProperty -Path $wl -Name Shell -ErrorAction Stop).Shell
if ($now -ne $want) { throw "shell value wrong: $now" }
& $exe update --silent --skip-checks --dir $dir --payload $payload
if ($LASTEXITCODE -ne 0) { throw 'full update failed' }
$now = (Get-ItemProperty -Path $wl -Name Shell -ErrorAction Stop).Shell
if ($now -ne $want) { throw "shell value lost on update: $now" }
cmd /c "`"$dir\restore.cmd`""
$mid = (Get-ItemProperty -Path $wl -Name Shell -ErrorAction SilentlyContinue).Shell
if ($mid -ne $before) { throw 'restore.cmd did not restore shell' }
& "$dir\uninstall.exe" uninstall --silent --purge
if ($LASTEXITCODE -ne 0) { throw 'full uninstall failed' }
for ($i = 0; $i -lt 40 -and ((Test-Path $dir) -or (Test-Path $key)); $i++) { Start-Sleep -Milliseconds 500 }
if (Test-Path $dir) { throw 'install dir not removed' }
if (Test-Path $key) { throw 'registry entry not removed' }
$after = (Get-ItemProperty -Path $wl -Name Shell -ErrorAction SilentlyContinue).Shell
if ($after -ne $before) { throw 'shell value not restored' }
if (Test-Path $backup) { throw 'shell backup left behind' }
