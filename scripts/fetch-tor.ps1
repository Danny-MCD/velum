<#
.SYNOPSIS
  Downloads the official Tor Expert Bundle for Windows and drops tor.exe
  (plus the DLLs it needs) into src-tauri/binaries/ using Tauri's sidecar
  naming convention, ready for `cargo tauri dev` / `cargo tauri build`.

.NOTES
  Verifies the download against Tor Project's detached GPG signature is
  intentionally NOT done here to keep this dependency-free; if you're
  building something you intend to distribute, verify the .asc signature
  against https://support.torproject.org/little-t-tor/verify-signature/
  before trusting the binary.
#>

param(
  [string]$Version = "15.0.20"
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$cacheDir = Join-Path $PSScriptRoot ".cache"
$binariesDir = Join-Path $root "src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $cacheDir, $binariesDir | Out-Null

$archiveName = "tor-expert-bundle-windows-x86_64-$Version.tar.gz"
$url = "https://dist.torproject.org/torbrowser/$Version/$archiveName"
$archivePath = Join-Path $cacheDir $archiveName
$extractDir = Join-Path $cacheDir "windows-x86_64-$Version"

if (-not (Test-Path $archivePath)) {
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archivePath
} else {
    Write-Host "Using cached $archivePath"
}

if (-not (Test-Path $extractDir)) {
    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
    Write-Host "Extracting..."
    tar -xzf $archivePath -C $extractDir
}

$torExe = Get-ChildItem -Path $extractDir -Recurse -Filter "tor.exe" | Select-Object -First 1
if (-not $torExe) {
    throw "Couldn't find tor.exe inside the extracted bundle at $extractDir"
}
$sourceDir = $torExe.Directory

$targetTriple = "x86_64-pc-windows-msvc"
Copy-Item $torExe.FullName (Join-Path $binariesDir "tor-$targetTriple.exe") -Force
Get-ChildItem -Path $sourceDir -Filter "*.dll" | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $binariesDir $_.Name) -Force
}

Write-Host "`nDone. Placed in $binariesDir :"
Get-ChildItem $binariesDir | ForEach-Object { Write-Host "  $($_.Name)" }
