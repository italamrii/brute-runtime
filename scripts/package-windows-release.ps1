#!/usr/bin/env pwsh
# Builds the polished, end-user-facing Windows release folder and ZIP
# from the already-built NSIS installer. This script only packages -
# it does not rebuild brute-desktop.exe itself. Run
# `npm run tauri build` (from desktop/) first, then this script.
#
# Usage (from anywhere):
#   pwsh scripts/package-windows-release.ps1
#
# Produces:
#   release/BRUTE-Runtime-v<version>-Windows-x64/
#     BRUTE-Runtime-Setup.exe
#     README-AR.txt
#     README-EN.txt
#     SHA256SUMS.txt
#     BRUTE-icon.ico
#     LICENSES/
#   release/BRUTE-Runtime-v<version>-Windows-x64.zip

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$desktopDir = Join-Path $repoRoot "desktop"
$tauriConfPath = Join-Path $desktopDir "src-tauri\tauri.conf.json"
$tauriConf = Get-Content $tauriConfPath -Raw | ConvertFrom-Json
$version = $tauriConf.version
$releaseName = "BRUTE-Runtime-v$version-Windows-x64"

$nsisDir = Join-Path $desktopDir "src-tauri\target\release\bundle\nsis"
$sourceInstaller = Get-ChildItem -Path $nsisDir -Filter "*-setup.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $sourceInstaller) {
    throw "No NSIS installer found in $nsisDir - run 'npm run tauri build' from desktop/ first."
}

$releaseRoot = Join-Path $repoRoot "release"
$releaseDir = Join-Path $releaseRoot $releaseName
$licensesDir = Join-Path $releaseDir "LICENSES"
$zipPath = Join-Path $releaseRoot "$releaseName.zip"

Write-Host "Packaging $releaseName from $($sourceInstaller.FullName) ..."

if (Test-Path $releaseDir) { Remove-Item -Recurse -Force $releaseDir }
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
New-Item -ItemType Directory -Path $licensesDir -Force | Out-Null

# Copy, never modify, the verified installer - checksum is computed
# after this copy/rename, against the exact bytes that ship.
$finalInstaller = Join-Path $releaseDir "BRUTE-Runtime-Setup.exe"
Copy-Item $sourceInstaller.FullName $finalInstaller -Force

# Official icon, included alongside the installer for convenience
# (the installer .exe itself already carries this icon as its own
# resource - this is a plain extra copy, not a folder-icon hack).
$iconSource = Join-Path $desktopDir "src-tauri\icons\icon.ico"
if (Test-Path $iconSource) {
    Copy-Item $iconSource (Join-Path $releaseDir "BRUTE-icon.ico") -Force
}

# Writes UTF-8 with no BOM - Windows PowerShell 5.1's own -Encoding UTF8
# always prepends a BOM, which would break a standard `sha256sum -c`
# verification against SHA256SUMS.txt.
function Write-Utf8NoBom($path, $content) {
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($path, $content, $utf8NoBom)
}

# SHA-256 of the final, renamed, unmodified file.
$hash = Get-FileHash $finalInstaller -Algorithm SHA256
$sha256Line = "$($hash.Hash.ToLower())  BRUTE-Runtime-Setup.exe`r`n"
Write-Utf8NoBom (Join-Path $releaseDir "SHA256SUMS.txt") $sha256Line
Write-Host "SHA-256: $($hash.Hash)"

$readmeEn = @"
BRUTE Runtime $version - Windows x64 (beta)
POWER. CONTROL. PERFORMANCE.
By Engineer Abdullah Alamri

HOW TO INSTALL
1. Double-click BRUTE-Runtime-Setup.exe.
2. Windows may show an "Unknown Publisher" (SmartScreen) warning. This
   is expected: this beta build is not yet code-signed with a
   commercial certificate. It does not mean the installer is unsafe -
   it means Windows cannot yet verify a publisher identity for it. You
   can verify the file yourself using the checksum below before running
   it if you'd like extra assurance.
3. To verify the installer's integrity, compare its SHA-256 against
   SHA256SUMS.txt in this folder:
     Get-FileHash BRUTE-Runtime-Setup.exe -Algorithm SHA256
   The result should match the hash listed in SHA256SUMS.txt.
4. Your own GGUF model files are never touched by uninstall. BRUTE only
   ever reads models you point it at or that it finds during an
   explicit device scan - it never copies or moves them by default, and
   uninstalling BRUTE does not delete any model file anywhere on your
   disk.

PRIVACY
BRUTE runs entirely on your device. No account, no cloud, no telemetry,
no automatic uploads. The only network activity BRUTE ever performs is
a model download you explicitly start yourself.

See LICENSES/ for third-party license notices for components bundled
with this installer (the CPU-only llama.cpp runtime).
"@

$readmeAr = @"
BRUTE Runtime $version - إصدار تجريبي لويندوز 64-bit
القوة. التحكم. الأداء.
بواسطة المهندس عبدالله العمري

طريقة التثبيت
1. اضغط مرتين على BRUTE-Runtime-Setup.exe.
2. قد يظهر لك تحذير من ويندوز بعنوان "ناشر غير معروف" (SmartScreen).
   هذا أمر متوقع: هذه نسخة تجريبية (Beta) لم تُوقَّع بعد بشهادة نشر
   تجارية. هذا لا يعني أن المُثبِّت غير آمن - فقط يعني أن ويندوز لا
   يمكنه بعد التحقق من هوية ناشر رسمية. يمكنك التحقق من الملف بنفسك
   باستخدام بصمة SHA-256 أدناه إن أردت مزيدًا من الاطمئنان.
3. للتحقق من سلامة المُثبِّت، قارن بصمة SHA-256 الخاصة به مع الملف
   SHA256SUMS.txt الموجود في هذا المجلد:
     Get-FileHash BRUTE-Runtime-Setup.exe -Algorithm SHA256
   يجب أن تطابق النتيجة البصمة المذكورة في SHA256SUMS.txt.
4. ملفات نماذج GGUF الخاصة بك لا تُحذف أبدًا عند إلغاء التثبيت. يقرأ
   BRUTE فقط النماذج التي تشير إليه بها أو التي يعثر عليها أثناء فحص
   صريح للجهاز - لا ينسخها أو ينقلها افتراضيًا، وإلغاء تثبيت BRUTE لا
   يحذف أي ملف نموذج من قرصك أبدًا.

الخصوصية
يعمل BRUTE بالكامل على جهازك. بدون حساب، بدون سحابة، بدون تتبّع، وبدون
رفع تلقائي لأي شيء. النشاط الشبكي الوحيد الذي يقوم به BRUTE هو تنزيل
نموذج تبدأه أنت بنفسك صراحةً.

راجع مجلد LICENSES/ للاطلاع على إشعارات تراخيص الأطراف الثالثة الخاصة
بالمكونات المرفقة مع هذا المُثبِّت (برنامج تشغيل llama.cpp لوحدة
المعالجة المركزية فقط).
"@

Write-Utf8NoBom (Join-Path $releaseDir "README-EN.txt") $readmeEn
Write-Utf8NoBom (Join-Path $releaseDir "README-AR.txt") $readmeAr

$llamaCppNotice = @"
llama.cpp (bundled CPU-only runtime)
Source: https://github.com/ggml-org/llama.cpp
Pinned release: b10064
License: MIT License

This installer bundles a CPU-only build of llama.cpp so BRUTE Runtime
works out of the box without requiring a separate download or manual
setup. llama.cpp is a third-party open-source project, not authored by
BRUTE Runtime's developers. This file is a notice, not a byte-for-byte
copy of llama.cpp's own LICENSE file - see the source repository above
for the authoritative license text.
"@

$bruteLicenseNotice = @"
BRUTE Runtime
Version: $version (beta)

This repository does not yet declare a formal open-source license for
BRUTE Runtime's own source code. This build is distributed as a beta
for evaluation. If you need clarity on usage terms for BRUTE Runtime
itself (as distinct from the third-party components it bundles, listed
separately in this folder), contact the project maintainers.
"@

Write-Utf8NoBom (Join-Path $licensesDir "llama.cpp-NOTICE.txt") $llamaCppNotice
Write-Utf8NoBom (Join-Path $licensesDir "BRUTE-Runtime-LICENSE-NOTICE.txt") $bruteLicenseNotice

# Zip the release folder's *contents* directly, so the installer sits
# at the top level of the archive (no extra folder to open first).
Compress-Archive -Path (Join-Path $releaseDir "*") -DestinationPath $zipPath -Force

Write-Host ""
Write-Host "Release folder: $releaseDir"
Write-Host "Release zip:    $zipPath"
Write-Host "SHA-256 (installer): $($hash.Hash)"
