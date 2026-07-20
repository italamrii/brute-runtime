# BRUTE Runtime — Branding assets

*(العربية أدناه / Arabic below)*

## The two source files

| File | Used for |
|---|---|
| `branding/brute-app-icon.png` | The **square symbol mark**. Source for every native app icon: the Windows `.exe`/taskbar/Start Menu/MSI/NSIS icon, macOS `.icns`, Linux desktop icon, and the compact sidebar mark inside the app. **Never** used as a horizontal logo. |
| `branding/brute-logo.png` | The **horizontal lockup** (symbol + "BRUTE" wordmark + the "POWER. CONTROL. PERFORMANCE." tagline). Used for brand placement inside the app (the About panel in Settings) and in documentation/README headers. **Never** used as the executable/window icon. |

Both files must stay at these exact paths and filenames — code and scripts
reference them by this exact name, not by content-based discovery.

## Requirements for a replacement image

- `brute-app-icon.png`: a **square** PNG, at least 512×512 (1024×1024 or
  larger preferred so it stays sharp when scaled up for macOS `.icns`).
  Should read clearly at small sizes (16/24/32/48/64px) since it becomes
  the taskbar and Start Menu icon.
- `brute-logo.png`: a wide/horizontal PNG (roughly 16:9 to 2:1), used at
  a maximum on-screen width of ~220px inside the app, so it doesn't need
  to be enormous — 1600px wide is plenty.
- Both are flattened PNGs (no transparency required, black/near-black
  background matches the app's own dark theme).

## Regenerating the native icons

After replacing `branding/brute-app-icon.png` with a new image, regenerate
every derived icon format with:

```powershell
pwsh desktop/scripts/generate-icons.ps1
```

This is a thin wrapper around Tauri's own icon generator:

```powershell
cd desktop
npx tauri icon ../branding/brute-app-icon.png -o src-tauri/icons
```

### What gets generated

Running the script regenerates, from the single square source image:

- `desktop/src-tauri/icons/icon.ico` — Windows `.exe`/taskbar/Start Menu/MSI/NSIS icon
- `desktop/src-tauri/icons/icon.icns` — macOS icon
- `desktop/src-tauri/icons/icon.png`, `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png` — Linux desktop icon + general PNG sizes
- `desktop/src-tauri/icons/Square*.png`, `StoreLogo.png` — Windows Store/Square logo variants (only relevant if a Store package target is ever added)
- `desktop/src/assets/brand/brute-app-icon.png` and `brute-logo.png` — refreshed copies the in-app UI (`desktop/src/config/brand.ts`) imports directly, **downscaled** (256px / 900px max dimension) since they're only ever displayed small in-app (~28px sidebar mark, ~220px About-panel logo) — the full-resolution originals stay in `branding/` only. The in-app `brute-logo.png` copy is also **auto-cropped to its content**: the source artwork has deliberate letterboxing (empty dark margin) for use as a wide hero banner, which reads as wasted space at in-app sizes, so the script trims to the actual logo's bounding box (plus a small padding margin) before resizing - same artwork, not redrawn or distorted, just tighter framing. The app-icon is never cropped (it's a full-bleed square glyph, not a banner).

None of these generated files should be hand-edited — always change the
two source files above and re-run the script.

## If the automatic generator isn't available

`npx tauri icon` requires the Tauri CLI (already a `desktop/` dev
dependency) and network-free local image processing — it does not call
out to any external service. If it's ever unavailable in some
environment, the release must not be blocked on it: replace the files
directly at these documented fallback locations and the app will pick
them up on the next `npm run build`:

- `desktop/src/assets/brand/brute-logo.png` (in-app horizontal logo)
- `desktop/src-tauri/icons/*` (native OS icons — a full pre-generated
  set must be provided manually if the generator can't run; at minimum
  `icon.ico` (Windows) and `icon.png` (Linux) are required for a build
  to produce correctly-branded installers)

## Where brand strings live

Product name, tagline, and brand colors are centralized in
`desktop/src/config/brand.ts` — never hardcode "BRUTE Runtime" or the
tagline text directly in a page component; import from there instead.

---

## أصول العلامة التجارية لـ BRUTE Runtime

### الملفان الأساسيان

| الملف | الاستخدام |
|---|---|
| `branding/brute-app-icon.png` | **الرمز المربع**. المصدر لكل أيقونة تطبيق أصلية: أيقونة exe/شريط المهام/قائمة ابدأ/MSI/NSIS على ويندوز، وأيقونة macOS، وأيقونة سطح مكتب Linux، والعلامة المدمجة في الشريط الجانبي داخل التطبيق. **لا يُستخدم أبدًا** كشعار أفقي. |
| `branding/brute-logo.png` | **الشعار الأفقي** (الرمز + كلمة "BRUTE" + شعار "POWER. CONTROL. PERFORMANCE."). يُستخدم لعرض العلامة التجارية داخل التطبيق (لوحة "حول" في الإعدادات) وفي رؤوس التوثيق. **لا يُستخدم أبدًا** كأيقونة تنفيذية أو نافذة. |

يجب أن يبقى الملفان بهذين المسارين والاسمين بالضبط — الكود والسكربتات تشير
إليهما بالاسم الدقيق، وليس عبر اكتشاف تلقائي بالمحتوى.

### متطلبات صورة بديلة

- `brute-app-icon.png`: صورة PNG **مربعة**، بحجم 512×512 على الأقل (يُفضّل
  1024×1024 أو أكبر لتبقى واضحة عند التكبير لأيقونة macOS). يجب أن تكون
  مقروءة بوضوح بالأحجام الصغيرة (16/24/32/48/64px).
- `brute-logo.png`: صورة PNG أفقية (بنسبة تقارب 16:9 إلى 2:1)، تُستخدم
  بعرض أقصى حوالي 220px داخل التطبيق.

### إعادة توليد الأيقونات الأصلية

بعد استبدال `branding/brute-app-icon.png` بصورة جديدة، أعد توليد كل صيغ
الأيقونات المشتقة بالأمر:

```powershell
pwsh desktop/scripts/generate-icons.ps1
```

### إذا تعذّر توليد الأيقونات تلقائيًا

لا يجوز أن يتوقف الإصدار على ذلك — استبدل الملفات مباشرة في المواقع
البديلة الموثّقة أعلاه (`desktop/src/assets/brand/brute-logo.png` و
`desktop/src-tauri/icons/*`)، وسيلتقطها التطبيق في عملية البناء التالية.

### أين تعيش نصوص العلامة التجارية

اسم المنتج والشعار وألوان العلامة التجارية مركزية في
`desktop/src/config/brand.ts` — لا تكتب "BRUTE Runtime" أو نص الشعار
مباشرة داخل أي صفحة؛ استورده من هناك بدلاً من ذلك.
