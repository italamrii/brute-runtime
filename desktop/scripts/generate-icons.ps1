#!/usr/bin/env pwsh
# Regenerates every native Tauri app icon (icon.ico, icon.icns, the PNG
# size set, and the Windows Store/Square logo set) from the official
# BRUTE app-icon source image. Run this after replacing
# branding/brute-app-icon.png with a new image - see branding/README.md.
#
# Usage (from anywhere):
#   pwsh desktop/scripts/generate-icons.ps1
#
# Equivalent manual command (from the desktop/ directory):
#   npx tauri icon ../branding/brute-app-icon.png -o src-tauri/icons

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$desktopDir = Join-Path $repoRoot "desktop"
$sourceIcon = Join-Path $repoRoot "branding\brute-app-icon.png"
$outputDir = Join-Path $desktopDir "src-tauri\icons"
$brandAssetIcon = Join-Path $desktopDir "src\assets\brand\brute-app-icon.png"
$brandAssetLogo = Join-Path $desktopDir "src\assets\brand\brute-logo.png"
$sourceLogo = Join-Path $repoRoot "branding\brute-logo.png"

if (-not (Test-Path $sourceIcon)) {
    throw "Source icon not found at $sourceIcon - see branding/README.md for the required filename and location."
}

Write-Host "Generating Tauri icons from $sourceIcon ..."
Push-Location $desktopDir
try {
    npx tauri icon $sourceIcon -o $outputDir
} finally {
    Pop-Location
}

# `tauri icon` also emits Android/iOS mipmap sets; BRUTE is a desktop-only
# app, so those are removed to keep the icons directory limited to what
# the Windows/macOS/Linux bundlers actually consume.
Remove-Item -Recurse -Force (Join-Path $outputDir "android") -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $outputDir "ios") -ErrorAction SilentlyContinue

Write-Host "Syncing in-app brand assets (sidebar/header/About use these directly) ..."
# Downscaled, not a raw copy of the full-resolution source: the sidebar
# mark renders at ~28px and the About-panel logo at ~220px, so shipping
# the original multi-megapixel PNGs would only bloat the installer.
Add-Type -AssemblyName System.Drawing
function Resize-BrandPng($srcPath, $dstPath, $maxDim) {
    $img = [System.Drawing.Image]::FromFile($srcPath)
    try {
        $ratio = [Math]::Min(1.0, $maxDim / [Math]::Max($img.Width, $img.Height))
        $w = [Math]::Max(1, [int]($img.Width * $ratio))
        $h = [Math]::Max(1, [int]($img.Height * $ratio))
        $bmp = New-Object System.Drawing.Bitmap $w, $h
        try {
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            try {
                $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $g.DrawImage($img, 0, 0, $w, $h)
            } finally { $g.Dispose() }
            $bmp.Save($dstPath, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $bmp.Dispose() }
    } finally { $img.Dispose() }
}
Resize-BrandPng $sourceIcon $brandAssetIcon 256
Resize-BrandPng $sourceLogo $brandAssetLogo 900

Write-Host "Done. Generated native icons in $outputDir and refreshed $desktopDir\src\assets\brand\ (downscaled for in-app use)."
