#!/usr/bin/env bash
# Lossless QIM: info → embed → capsule-only extract → sidecar extract
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

"$BIN" info -i "$CARRIER" -s qim -c mid --message-bytes 32
"$BIN" embed -i "$CARRIER" --message-text 'hello' -o "$WORK/loaded.wav" \
  -s qim -c mid --sidecar "$WORK/loaded.json" --key 'shared-secret'
"$BIN" extract -i "$WORK/loaded.wav" -o "$WORK/from-capsule.txt" --key 'shared-secret'
"$BIN" extract -i "$WORK/loaded.wav" -o "$WORK/from-sidecar.txt" \
  --sidecar "$WORK/loaded.json" --key 'shared-secret'
echo "capsule: $(cat "$WORK/from-capsule.txt")"
echo "sidecar: $(cat "$WORK/from-sidecar.txt")"
