#!/usr/bin/env bash
# Encrypted payload (ChaCha20-Poly1305) + capsule-only extract
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARRIER="$ROOT/testdata/music/carrier-music.flac"
WORK="${TMPDIR:-/tmp}/audiostego-examples"
mkdir -p "$WORK"

if [[ ! -f "$CARRIER" ]]; then
  echo "missing $CARRIER — run ./scripts/clip-testdata.sh" >&2
  exit 2
fi

resolve_bin() {
  local td="${CARGO_TARGET_DIR:-$ROOT/target}"
  if [[ -x "$td/release/audiostego" ]]; then
    echo "$td/release/audiostego"
  elif [[ -x "$td/debug/audiostego" ]]; then
    echo "$td/debug/audiostego"
  elif [[ -x "$ROOT/target/release/audiostego" ]]; then
    echo "$ROOT/target/release/audiostego"
  elif [[ -x "$ROOT/target/debug/audiostego" ]]; then
    echo "$ROOT/target/debug/audiostego"
  elif command -v audiostego >/dev/null 2>&1; then
    command -v audiostego
  else
    echo "audiostego binary not found; run: cargo build" >&2
    exit 1
  fi
}
BIN="$(resolve_bin)"

"$BIN" embed -i "$CARRIER" --message-text 'secret' -o "$WORK/enc.wav" \
  -s qim -c mid --key 'shared-secret' --encrypt --sidecar "$WORK/enc.json"
"$BIN" extract -i "$WORK/enc.wav" -o "$WORK/enc-out.txt" --key 'shared-secret'
echo "recovered: $(cat "$WORK/enc-out.txt")"
