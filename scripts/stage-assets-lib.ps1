# The manifest-carry half of stage-assets.ps1, split out so it can be tested
# without staging files or running ExifTool.
#
# Regenerating assets.json from the staged file list must NEVER drop an entry
# the manifest already carries: non-optional models outside the staged list
# (ocec_l, sface, the DINOv2 tower, the nima placeholder) and opt-in downloads
# (the decision model, the built-in vision model) are pinned by hand or by an
# earlier run, and silently losing them breaks installs that rely on them.

function Add-CarriedAssets {
  param(
    # The freshly staged entries (ordered hashtables: name, dest, ...).
    [object[]]$Staged,
    # Path to the existing assets.json (may not exist on a first run).
    [string]$ExistingPath
  )
  $out = [System.Collections.ArrayList]::new()
  foreach ($s in $Staged) { [void]$out.Add($s) }
  if (-not (Test-Path $ExistingPath)) { return ,$out.ToArray() }
  $existing = (Get-Content $ExistingPath -Raw | ConvertFrom-Json).assets
  if (-not $existing) { return ,$out.ToArray() }
  $stagedNames = @($Staged | ForEach-Object { $_['name'] })
  $stagedExiftool = @($Staged | Where-Object { $_['dest'] -eq 'exiftool' }).Count -gt 0
  foreach ($kept in $existing) {
    if ($stagedNames -contains $kept.name) { continue } # re-staged above wins
    # A re-zipped ExifTool carries a new version/hash: the old zip entry is
    # replaced, never kept beside the new one.
    if ($stagedExiftool -and $kept.dest -eq 'exiftool') { continue }
    # Carry every field over unchanged (optional, note, _comment, sources...).
    $h = [ordered]@{}
    foreach ($p in $kept.PSObject.Properties) { $h[$p.Name] = $p.Value }
    [void]$out.Add($h)
  }
  return ,$out.ToArray()
}
