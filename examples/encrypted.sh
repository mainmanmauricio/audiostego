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

# shellcheck source=resolve-bin.sh
. "$(dirname "$0")/resolve-bin.sh"
BIN="$(resolve_bin)"

"$BIN" embed -i "$CARRIER" --message-text 'secret' -o "$WORK/enc.wav" \
  -s qim -c mid --key 'shared-secret' --encrypt --sidecar "$WORK/enc.json"
"$BIN" extract -i "$WORK/enc.wav" -o "$WORK/enc-out.txt" --key 'shared-secret'
echo "recovered: $(cat "$WORK/enc-out.txt")"
