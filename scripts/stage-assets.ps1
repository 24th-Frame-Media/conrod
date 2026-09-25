<#
Maintainer step: collect the model files and ExifTool that Conrod was TESTED with,
pin them by SHA-256 in assets.json (committed), and populate the app's resources/
folder for a local build.

    powershell -File scripts/stage-assets.ps1

Drop the OCR models (ch_PP-OCRv4_det_infer.onnx, ch_PP-OCRv4_rec_infer.onnx) and
the other pinned models into %USERPROFILE%\.conrod\models first; this script
never downloads them itself, it only stages and hashes what's already local.

Then publish dist/assets-v1/* as a GitHub release named assets-v1 (see
RELEASING.md); CI's fetch-release-assets.ps1 downloads from there and refuses
anything whose hash differs from assets.json.
#>
param(
  [string]$ExifToolDir = (Join-Path $env:LOCALAPPDATA 'Programs\ExifTool')
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$out = Join-Path $root 'dist\assets-v1'
$res = Join-Path $root 'crates\conrod-app\resources'
$models = Join-Path $env:USERPROFILE '.conrod\models'
$cache = Join-Path $env:USERPROFILE '.cache'
$releaseUrl = 'https://github.com/kapsikkum/conrod/releases/download/assets-v1'
New-Item -ItemType Directory -Force $out, (Join-Path $res 'models'), (Join-Path $res 'exiftool') | Out-Null

# name, source path, licence
$files = @(
  @('yolo11s-960.onnx', "$models\yolo11s-960.onnx", 'AGPL-3.0 (Ultralytics YOLO11)'),
  @('face_detection_yunet_2023mar.onnx', "$models\face_detection_yunet_2023mar.onnx", 'MIT (OpenCV Zoo YuNet)'),
  @('dinov2-small-quantized.onnx', "$models\dinov2-small-quantized.onnx", 'Apache-2.0 (Meta DINOv2)'),
  @('yolo-v9-t-640-license-plates-end2end.onnx', "$cache\open-image-models\yolo-v9-t-640-license-plate-end2end\yolo-v9-t-640-license-plates-end2end.onnx", 'MIT (open-image-models)'),
  @('global_mobile_vit_v2_ocr.onnx', "$cache\fast-plate-ocr\global-plates-mobile-vit-v2-model\global_mobile_vit_v2_ocr.onnx", 'MIT (fast-plate-ocr)'),
  @('ch_PP-OCRv4_det_infer.onnx', "$models\ch_PP-OCRv4_det_infer.onnx", 'Apache-2.0 (PaddleOCR via RapidOCR)'),
  @('ch_PP-OCRv4_rec_infer.onnx', "$models\ch_PP-OCRv4_rec_infer.onnx", 'Apache-2.0 (PaddleOCR via RapidOCR)')
)
# Upstream URLs, where the model has one; the release mirror ($releaseUrl) covers
# the rest (the OCR models below, which upstream only ships bundled in a package,
# not as a standalone downloadable file). A download only counts if its SHA-256
# equals the pin.
$upstream = @{
  'face_detection_yunet_2023mar.onnx' = @('https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx')
  'dinov2-small-quantized.onnx' = @('https://huggingface.co/Xenova/dinov2-small/resolve/main/onnx/model_quantized.onnx')
  'yolo-v9-t-640-license-plates-end2end.onnx' = @('https://github.com/ankandrew/open-image-models/releases/download/assets/yolo-v9-t-640-license-plates-end2end.onnx')
  'global_mobile_vit_v2_ocr.onnx' = @('https://github.com/ankandrew/cnn-ocr-lp/releases/download/arg-plates/global_mobile_vit_v2_ocr.onnx')
  'ch_PP-OCRv4_det_infer.onnx' = @("$releaseUrl/ch_PP-OCRv4_det_infer.onnx")
  'ch_PP-OCRv4_rec_infer.onnx' = @("$releaseUrl/ch_PP-OCRv4_rec_infer.onnx")
}
$assets = @()
foreach ($f in $files) {
  if (-not (Test-Path $f[1])) { throw "missing source for $($f[0]): $($f[1])" }
  Copy-Item $f[1] (Join-Path $out $f[0]) -Force
  Copy-Item $f[1] (Join-Path $res "models\$($f[0])") -Force
  $assets += [ordered]@{ name = $f[0]; dest = 'models'; extract = $false; licence = $f[2]; sources = @($upstream[$f[0]]) }
}
# ExifTool: the folder as installed (exe + exiftool_files, which carries its licences).
$ver = (& (Join-Path $ExifToolDir 'ExifTool.exe') -ver).Trim()
$zip = "exiftool-$ver-win64.zip"
$zipPath = Join-Path $out $zip
if (Test-Path $zipPath) { Remove-Item $zipPath }
Compress-Archive -Path (Join-Path $ExifToolDir '*') -DestinationPath $zipPath
Copy-Item (Join-Path $ExifToolDir '*') (Join-Path $res 'exiftool') -Recurse -Force
$assets += [ordered]@{ name = $zip; dest = 'exiftool'; extract = $true; licence = 'Artistic-1.0-Perl OR GPL-1.0-or-later (ExifTool by Phil Harvey; Perl runtime licences in exiftool_files)'; sources = @() }

foreach ($a in $assets) {
  $p = Join-Path $out $a.name
  $a['sha256'] = (Get-FileHash $p -Algorithm SHA256).Hash.ToLower()
  $a['size'] = (Get-Item $p).Length
}
$manifest = [ordered]@{ release = $releaseUrl; assets = $assets }
$manifest | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $PSScriptRoot 'assets.json') -Encoding UTF8
"staged $($assets.Count) assets -> $out ; manifest scripts/assets.json ; resources populated"
