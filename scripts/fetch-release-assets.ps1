<#
CI step: download the assets that ship IN the installer (ExifTool only - the ONNX
models are not bundled; the app downloads and verifies them itself on first run,
see crates/conrod-io/src/assets.rs) into the app's resources/ folder.
FAIL CLOSED: an asset is only used if its SHA-256 equals the pinned value.

    powershell -File scripts/fetch-release-assets.ps1
#>
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$res = Join-Path $root 'crates\conrod-app\resources'
$manifest = Get-Content (Join-Path $PSScriptRoot 'assets.json') -Raw | ConvertFrom-Json
$tmp = Join-Path ([IO.Path]::GetTempPath()) "conrod-assets-$PID"
New-Item -ItemType Directory -Force $tmp | Out-Null
foreach ($a in $manifest.assets | Where-Object { $_.dest -ne 'models' }) {
  if (-not $a.sha256) { throw "$($a.name): no pinned sha256 in assets.json" }
  $dest = Join-Path $res $a.dest
  New-Item -ItemType Directory -Force $dest | Out-Null
  $file = Join-Path $tmp $a.name
  Invoke-WebRequest -Uri "$($manifest.release)/$($a.name)" -OutFile $file -UseBasicParsing
  $got = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
  if ($got -ne $a.sha256) { throw "$($a.name): sha256 $got does not match the pinned $($a.sha256)" }
  if ($a.extract) { Expand-Archive $file -DestinationPath $dest -Force } else { Copy-Item $file $dest -Force }
  "ok  $($a.name)"
}
Remove-Item $tmp -Recurse -Force
