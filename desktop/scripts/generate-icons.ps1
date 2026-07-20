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

# `brute-logo.png` ships with deliberate letterboxing (empty dark margin)
# baked into the source artwork for use as a wide hero banner. Displayed
# small in-app (the About panel), that margin reads as wasted space, so
# the in-app copy is auto-cropped to the actual logo content (same
# artwork, not redrawn/distorted - just trimmed) before resizing. The
# app-icon is intentionally left uncropped: it's a full-bleed square
# glyph, not a letterboxed banner.
function Crop-ToContent($bmp, [double]$luminanceThreshold = 30, [double]$padXFrac = 0.06, [double]$padYFrac = 0.15) {
    $w = $bmp.Width; $h = $bmp.Height
    $minX = $w; $minY = $h; $maxX = 0; $maxY = 0
    $step = 2
    for ($y = 0; $y -lt $h; $y += $step) {
        for ($x = 0; $x -lt $w; $x += $step) {
            $px = $bmp.GetPixel($x, $y)
            $lum = 0.299 * $px.R + 0.587 * $px.G + 0.114 * $px.B
            if ($lum -gt $luminanceThreshold) {
                if ($x -lt $minX) { $minX = $x }
                if ($x -gt $maxX) { $maxX = $x }
                if ($y -lt $minY) { $minY = $y }
                if ($y -gt $maxY) { $maxY = $y }
            }
        }
    }
    if ($maxX -le $minX -or $maxY -le $minY) {
        # Nothing above threshold (shouldn't happen for real artwork) -
        # fall back to the full image rather than an empty crop.
        return New-Object System.Drawing.Rectangle 0, 0, $w, $h
    }
    $boxW = $maxX - $minX; $boxH = $maxY - $minY
    $padX = [int]($boxW * $padXFrac)
    $padY = [int]($boxH * $padYFrac)
    $cropX = [Math]::Max(0, $minX - $padX)
    $cropY = [Math]::Max(0, $minY - $padY)
    $cropRight = [Math]::Min($w, $maxX + $padX)
    $cropBottom = [Math]::Min($h, $maxY + $padY)
    return New-Object System.Drawing.Rectangle $cropX, $cropY, ($cropRight - $cropX), ($cropBottom - $cropY)
}

function Resize-BrandPng($srcPath, $dstPath, $maxDim, [switch]$CropToContent) {
    $img = [System.Drawing.Bitmap]::FromFile($srcPath)
    try {
        $sourceRect = New-Object System.Drawing.Rectangle 0, 0, $img.Width, $img.Height
        if ($CropToContent) {
            $sourceRect = Crop-ToContent $img
        }
        $ratio = [Math]::Min(1.0, $maxDim / [Math]::Max($sourceRect.Width, $sourceRect.Height))
        $w = [Math]::Max(1, [int]($sourceRect.Width * $ratio))
        $h = [Math]::Max(1, [int]($sourceRect.Height * $ratio))
        $bmp = New-Object System.Drawing.Bitmap $w, $h
        try {
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            try {
                $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $g.DrawImage($img, (New-Object System.Drawing.Rectangle 0, 0, $w, $h), $sourceRect, [System.Drawing.GraphicsUnit]::Pixel)
            } finally { $g.Dispose() }
            $bmp.Save($dstPath, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $bmp.Dispose() }
    } finally { $img.Dispose() }
}
Resize-BrandPng $sourceIcon $brandAssetIcon 256
Resize-BrandPng $sourceLogo $brandAssetLogo 900 -CropToContent

Write-Host "Done. Generated native icons in $outputDir and refreshed $desktopDir\src\assets\brand\ (downscaled for in-app use, logo auto-cropped to content)."
