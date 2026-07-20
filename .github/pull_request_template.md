## What does this change?

<!-- One or two sentences: what and why. -->

## Checklist

- [ ] I've read [CONTRIBUTING.md](../CONTRIBUTING.md) (note: code
      contributions may not be mergeable yet pending the license
      decision — see [LICENSE-NOTICE.md](../LICENSE-NOTICE.md)).
- [ ] Frontend: `npm run lint`, `npx tsc --noEmit`, `npm test`, `npm run build` all pass.
- [ ] Rust (root + `desktop/src-tauri`): `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` all pass.
- [ ] No secrets, absolute developer paths, generated binaries, or model
      files are included in this diff.
- [ ] If this touches network-capable code, I've confirmed it doesn't
      add a new always-on/background network path (see
      [PRIVACY.md](../PRIVACY.md) and [SECURITY.md](../SECURITY.md)).
- [ ] Documentation updated if this changes user-facing behavior.

## Testing performed

<!-- What did you actually run/verify? Installed-build testing for anything touching packaging/process launch/UI navigation is strongly preferred over dev-mode only. -->
