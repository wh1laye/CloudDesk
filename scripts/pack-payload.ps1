param([Parameter(Mandatory)][string]$Payload,[Parameter(Mandatory)][string]$Out)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path $Payload).Path
$files = @(Get-ChildItem $root -File -Recurse)
if ($files.Count -eq 0) { throw 'payload is empty' }
$stream = [System.IO.File]::Create($Out)
$w = New-Object System.IO.BinaryWriter($stream)
try {
  $w.Write([System.Text.Encoding]::ASCII.GetBytes('CDP1'))
  $w.Write([uint32]$files.Count)
  foreach ($f in $files) {
    $rel = $f.FullName.Substring($root.Length + 1).Replace('\', '/')
    $name = [System.Text.Encoding]::UTF8.GetBytes($rel)
    $w.Write([uint16]$name.Length)
    $w.Write($name)
    $w.Write([uint64]$f.Length)
    $w.Write([System.IO.File]::ReadAllBytes($f.FullName))
  }
} finally {
  $w.Dispose()
}
