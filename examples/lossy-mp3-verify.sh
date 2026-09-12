#!/usr/bin/env bash
# Best-effort MP3 verify (uses sidecar inside verify; needs ffmpeg)
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARRIER="$ROOT/testdata/music/carrier-music.flac"

if [[ ! -f "$CARRIER" ]]; then
  echo "missing $CARRIER — run ./scripts/clip-testdata.sh" >&2
  exit 2
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not on PATH; skipping lossy verify" >&2
  exit 0
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

"$BIN" verify -i "$CARRIER" --message-text 'ok' --lossy mp3 --output-format mp3
