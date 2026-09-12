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

# shellcheck source=resolve-bin.sh
. "$(dirname "$0")/resolve-bin.sh"
BIN="$(resolve_bin)"

"$BIN" verify -i "$CARRIER" --message-text 'ok' --lossy mp3 --output-format mp3
