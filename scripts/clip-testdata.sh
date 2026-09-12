#!/usr/bin/env bash
# Download Drozerix "4 RNDD!" (CC0) and clip ~15 s into testdata/music/carrier-music.flac
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/testdata/music/carrier-music.flac"
TMP="${TMPDIR:-/tmp}/audiostego-drozerix-$$.flac"
# Quote the '!' in the Commons filename.
URL='https://commons.wikimedia.org/wiki/Special:FilePath/Drozerix_-_4_RNDD!.flac'

mkdir -p "$(dirname "$OUT")"
echo "Downloading $URL ..."
curl -fsSL -o "$TMP" "$URL"
ffprobe -v error -show_entries stream=channels,sample_rate,duration \
  -of default=noprint_wrappers=1 "$TMP"
ffmpeg -y -i "$TMP" -t 15 -c:a flac "$OUT"
rm -f "$TMP"
(
  cd "$ROOT/testdata"
  sha256sum "music/carrier-music.flac" > checksums.sha256
)
echo "Wrote $OUT"
cat "$ROOT/testdata/checksums.sha256"
ffprobe -v error -show_entries stream=channels,sample_rate,duration \
  -of default=noprint_wrappers=1 "$OUT"
