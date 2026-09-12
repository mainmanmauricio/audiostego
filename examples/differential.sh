#!/usr/bin/env bash
# Differential strategy (requires --original on extract)
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

"$BIN" embed -i "$CARRIER" --message-text 'secret' -o "$WORK/diff.wav" \
  -s differential --key 'shared-secret'
"$BIN" extract -i "$WORK/diff.wav" --original "$CARRIER" -o "$WORK/diff-out.txt" \
  --key 'shared-secret'
echo "recovered: $(cat "$WORK/diff-out.txt")"
