<#
.SYNOPSIS
    Downloads a pinned, verified llama.cpp Windows release binary for use
    with `brute benchmark` / `brute recommend`.

.DESCRIPTION
    This script is the ONLY place in this project that reaches out to the
    network to fetch a third-party executable. `brute` itself never
    downloads anything - it only launches a binary path it is given, after
    checking it against a local pin.

    Steps:
      1. Reads scripts/llama-cpp-manifest.json (pinned tag/asset/sha256).
      2. Downloads the requested backend's release zip.
      3. Verifies the downloaded zip's SHA-256 against the pinned value
         BEFORE extracting anything. Aborts on mismatch.
      4. Extracts to .tools/llama.cpp/<tag>/<backend>/.
      5. Computes SHA-256 of each entry-point exe and writes a sibling
         <exe>.sha256 pin file, which `brute` checks before every launch.

.PARAMETER Backend
    Which backend build to fetch: "cpu" (default) or "vulkan".

.EXAMPLE
    .\scripts\fetch-llama-cpp.ps1
    .\scripts\fetch-llama-cpp.ps1 -Backend vulkan
#>
[CmdletBinding()]
param(
    [ValidateSet("cpu", "vulkan")]
    [string]$Backend = "cpu"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $PSScriptRoot "llama-cpp-manifest.json"
$manifest = Get-Content -Raw -Path $manifestPath | ConvertFrom-Json

$entry = $manifest.binaries | Where-Object { $_.backend -eq $Backend } | Select-Object -First 1
if (-not $entry) {
    throw "No manifest entry for backend '$Backend'"
}

$toolsDir = Join-Path $repoRoot ".tools\llama.cpp\$($entry.tag)\$($entry.backend)"
New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null

$zipPath = Join-Path $toolsDir $entry.asset
Write-Host "Downloading $($entry.asset) ($($entry.tag), $($entry.backend))..."
Invoke-WebRequest -Uri $entry.download_url -OutFile $zipPath -UseBasicParsing

Write-Host "Verifying SHA-256 against pinned manifest value..."
$actualHash = (Get-FileHash -Algorithm SHA256 -Path $zipPath).Hash.ToLower()
$expectedHash = $entry.sha256_zip.ToLower()

if ($actualHash -ne $expectedHash) {
    Remove-Item -Force $zipPath
    throw "SHA-256 MISMATCH for $($entry.asset): expected $expectedHash, got $actualHash. Downloaded file was deleted. Refusing to extract or execute an unverified binary."
}
Write-Host "SHA-256 verified: $actualHash"

Write-Host "Extracting to $toolsDir..."
Expand-Archive -Path $zipPath -DestinationPath $toolsDir -Force
Remove-Item -Force $zipPath

foreach ($exeName in $entry.entry_points) {
    $matches = Get-ChildItem -Path $toolsDir -Filter $exeName -Recurse -ErrorAction SilentlyContinue
    if (-not $matches -or $matches.Count -eq 0) {
        Write-Warning "Entry point $exeName not found after extraction - the release archive layout may have changed."
        continue
    }
    $exePath = $matches[0].FullName
    $exeHash = (Get-FileHash -Algorithm SHA256 -Path $exePath).Hash.ToLower()
    Set-Content -Path "$exePath.sha256" -Value $exeHash -NoNewline
    Write-Host "Pinned $exePath -> $exeHash"
}

Write-Host ""
Write-Host "Done. Point brute at this directory, e.g.:"
Write-Host "  cargo run -- benchmark --model <path-to-model.gguf> --llama-bin `"$toolsDir`" --backend $Backend"
