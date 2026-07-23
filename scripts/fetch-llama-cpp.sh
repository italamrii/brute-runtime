#!/usr/bin/env bash
#
# Downloads a pinned, verified llama.cpp macOS release binary for use with
# `brute benchmark` / `brute recommend` and for bundling into the desktop app.
#
# This is the macOS/Unix counterpart of scripts/fetch-llama-cpp.ps1. Like that
# script, it is the ONLY place in this project that reaches out to the network
# to fetch a third-party executable. `brute` itself never downloads anything -
# it only launches a binary path it is given, after checking it against a local
# pin (the sibling <exe>.sha256 files this script writes).
#
# Steps:
#   1. Reads scripts/llama-cpp-manifest.json (pinned tag/asset/sha256).
#   2. Downloads the requested macOS arch's release tarball.
#   3. Verifies the downloaded tarball's SHA-256 against the pinned value
#      BEFORE extracting anything. Aborts on mismatch.
#   4. Extracts to .tools/llama.cpp/<tag>/cpu/ (flattened - the macOS tarball
#      nests everything under a llama-<tag>/ dir, but the desktop app and the
#      engine's `llama_cli_path`/`llama_bench_path` expect the binaries and
#      their sibling .dylibs directly in the runtime dir).
#   5. Computes SHA-256 of each entry-point binary and writes a sibling
#      <exe>.sha256 pin file, which `brute` checks before every launch.
#
# Usage:
#   ./scripts/fetch-llama-cpp.sh              # auto-detect arch (arm64/x64)
#   ./scripts/fetch-llama-cpp.sh arm64
#   ./scripts/fetch-llama-cpp.sh x64

set -euo pipefail

ARCH="${1:-}"
if [[ -z "$ARCH" ]]; then
  case "$(uname -m)" in
    arm64|aarch64) ARCH="arm64" ;;
    x86_64)        ARCH="x64" ;;
    *) echo "Unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
  esac
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
MANIFEST="$SCRIPT_DIR/llama-cpp-manifest.json"

read_entry() {
  # $1 = jq-style field; uses python3 since jq may not be installed.
  python3 - "$MANIFEST" "$ARCH" "$1" <<'PY'
import json, sys
manifest, arch, field = sys.argv[1], sys.argv[2], sys.argv[3]
data = json.load(open(manifest))
for b in data["binaries"]:
    if b.get("os") == "macos" and b.get("arch") == arch:
        print(b[field])
        break
else:
    sys.exit(f"No manifest entry for macos/{arch}")
PY
}

TAG="$(read_entry tag)"
ASSET="$(read_entry asset)"
URL="$(read_entry download_url)"
EXPECTED="$(read_entry sha256_zip)"

TOOLS_DIR="$REPO_ROOT/.tools/llama.cpp/$TAG/cpu"
mkdir -p "$TOOLS_DIR"
TARBALL="$TOOLS_DIR/$ASSET"

echo "Downloading $ASSET ($TAG, macos/$ARCH)..."
curl -sSL -o "$TARBALL" "$URL"

echo "Verifying SHA-256 against pinned manifest value..."
ACTUAL="$(shasum -a 256 "$TARBALL" | awk '{print $1}')"
if [[ "$ACTUAL" != "$EXPECTED" ]]; then
  rm -f "$TARBALL"
  echo "SHA-256 MISMATCH for $ASSET: expected $EXPECTED, got $ACTUAL." >&2
  echo "Downloaded file was deleted. Refusing to extract or execute an unverified binary." >&2
  exit 1
fi
echo "SHA-256 verified: $ACTUAL"

echo "Extracting (flattened) to $TOOLS_DIR..."
# --strip-components=1 drops the leading llama-<tag>/ directory so binaries and
# their .dylibs land directly in the runtime dir.
tar -xzf "$TARBALL" -C "$TOOLS_DIR" --strip-components=1
rm -f "$TARBALL"

for exe in llama-cli llama-bench; do
  exe_path="$TOOLS_DIR/$exe"
  if [[ ! -f "$exe_path" ]]; then
    echo "Warning: entry point $exe not found after extraction - archive layout may have changed." >&2
    continue
  fi
  h="$(shasum -a 256 "$exe_path" | awk '{print $1}')"
  printf "%s" "$h" > "$exe_path.sha256"
  echo "Pinned $exe_path -> $h"
done

echo ""
echo "Done. The desktop app bundles this dir automatically via tauri.conf.json."
echo "Or point brute at it directly, e.g.:"
echo "  cargo run -- benchmark --model <path-to-model.gguf> --llama-bin \"$TOOLS_DIR\" --backend cpu"
