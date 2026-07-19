# BRUTE Runtime Desktop

Windows-first Tauri + React + TypeScript shell for the BRUTE Runtime engine.

**بياناتك ما تطلع من جهازك — Your data never leaves your device.**

## Visual system

Premium near-black / charcoal systems console with deep-red instrumentation accents.
Design tokens: `src/styles/tokens.css`. Shared layout primitives: `src/styles/global.css`.

## Pages

Overview · Hardware · Models · Optimize · Run · Profiles · Health · Settings · Onboarding

## Development

```bash
cd desktop
npm install
npm run tauri dev
```

## Validation

```bash
npm test
npm run lint
npm run build
cd src-tauri && cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings
npm run tauri build
```

## Privacy

No fetch/axios/telemetry SDKs in the frontend. Official model URLs exist only as inert catalog metadata.
All IPC goes through typed wrappers in `src/lib/api.ts`.

## Known limitations

- Windows only for Stage 4 packaging
- Unsigned development builds (SmartScreen may warn)
- No automatic updater, accounts, cloud sync, or model downloads
- CUDA/Vulkan “verified” requires a successful load/benchmark — detection alone is not verification
