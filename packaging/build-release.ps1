# FARO QEMU MANAGER (C) R.Trunk 2026
#
# Builds the two things a release ships: a portable ZIP and an MSI
# installer. Run it from anywhere:
#
#     pwsh packaging/build-release.ps1
#
# Everything lands in dist/. The version is read from Cargo.toml, never
# typed here, so the file names, the MSI version and the program itself
# can never disagree.
#
# Needs: Rust toolchain, and WiX 7 (dotnet tool install --global wix).

#Requires -Version 7
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$packaging = Join-Path $root 'packaging'
$dist = Join-Path $root 'dist'

# --- Version, from the one place that owns it -------------------------------

$cargo = Get-Content (Join-Path $root 'Cargo.toml') -Raw
if ($cargo -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw "No version found in Cargo.toml"
}
$version = $Matches[1]
# MSI versions are four-part; the build number stays zero, matching the
# Windows resource that build.rs compiles in.
$msiVersion = "$version.0"
Write-Host "FARO QEMU MANAGER $version" -ForegroundColor Cyan

# --- Build ------------------------------------------------------------------

Write-Host "`n[1/4] cargo build --release" -ForegroundColor Yellow
Push-Location $root
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$exe = Join-Path $root 'target\release\faro-qem.exe'
if (-not (Test-Path $exe)) { throw "Executable not found: $exe" }

# The executable must agree with Cargo.toml. A stale binary from an
# earlier version would otherwise be shipped under the new name.
$fileVersion = (Get-Item $exe).VersionInfo.FileVersion
if ($fileVersion -ne $msiVersion) {
    throw "The built executable reports $fileVersion, but Cargo.toml says $version (expected $msiVersion). Check build.rs."
}
Write-Host "      executable reports $fileVersion" -ForegroundColor DarkGray

if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $dist | Out-Null

# --- Portable ZIP -----------------------------------------------------------

Write-Host "`n[2/4] portable ZIP" -ForegroundColor Yellow
$stageName = "FARO-QEMU-MANAGER-$version-portable"
$stage = Join-Path $dist $stageName
New-Item -ItemType Directory -Path $stage | Out-Null

Copy-Item $exe $stage
Copy-Item (Join-Path $packaging 'portable.txt') $stage
Copy-Item (Join-Path $root 'LICENSE') (Join-Path $stage 'LICENSE.txt')
Copy-Item (Join-Path $root 'README.md') $stage

$zip = Join-Path $dist "$stageName.zip"
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
Remove-Item $stage -Recurse -Force
Write-Host ("      {0} ({1:N1} MB)" -f (Split-Path $zip -Leaf), ((Get-Item $zip).Length / 1MB)) -ForegroundColor DarkGray

# --- Licence as RTF, for the installer's licence page -----------------------

Write-Host "`n[3/4] licence as RTF" -ForegroundColor Yellow
$rtf = Join-Path $dist 'LICENSE.rtf'
$text = Get-Content (Join-Path $root 'LICENSE') -Raw
# RTF treats these three as control characters, and non-ASCII has to go.
$text = $text -replace '\\', '\\\\' -replace '\{', '\{' -replace '\}', '\}'
$lines = $text -split "`r?`n" | ForEach-Object {
    ($_ -replace '[^\x00-\x7F]', '?') + '\par'
}
$body = $lines -join "`r`n"
@"
{\rtf1\ansi\deff0{\fonttbl{\f0\fnil\fcharset0 Segoe UI;}}
\fs16
$body
}
"@ | Set-Content -Path $rtf -Encoding ascii
Write-Host ("      LICENSE.rtf ({0:N0} KB)" -f ((Get-Item $rtf).Length / 1KB)) -ForegroundColor DarkGray

# --- MSI installer ----------------------------------------------------------

Write-Host "`n[4/4] MSI installer" -ForegroundColor Yellow
$msi = Join-Path $dist "FARO-QEMU-MANAGER-$version-setup.msi"

# The UI extension carries the licence page. Adding it is idempotent.
wix extension add -g WixToolset.UI.wixext | Out-Null

wix build `
    -arch x64 `
    -ext WixToolset.UI.wixext `
    -d "Version=$msiVersion" `
    -d "ExeFile=$exe" `
    -d "IconFile=$(Join-Path $root 'assets\icon.ico')" `
    -d "LicenseFile=$(Join-Path $root 'LICENSE')" `
    -d "LicenseRtf=$rtf" `
    -d "ReadmeFile=$(Join-Path $root 'README.md')" `
    -o $msi `
    (Join-Path $packaging 'faro-qem.wxs')

if ($LASTEXITCODE -ne 0) { throw "wix build failed" }
Remove-Item $rtf -Force
# The .wixpdb is build debug information, not something anyone downloads.
Get-ChildItem $dist -Filter '*.wixpdb' | Remove-Item -Force
Write-Host ("      {0} ({1:N1} MB)" -f (Split-Path $msi -Leaf), ((Get-Item $msi).Length / 1MB)) -ForegroundColor DarkGray

# --- Checksums, so a download can be verified -------------------------------

$sums = Join-Path $dist 'SHA256SUMS.txt'
Get-ChildItem $dist -File | Where-Object { $_.Name -ne 'SHA256SUMS.txt' } | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
} | Set-Content $sums -Encoding ascii

Write-Host "`nDone. In dist/:" -ForegroundColor Green
Get-ChildItem $dist -File | ForEach-Object {
    Write-Host ("  {0,-48} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB))
}
