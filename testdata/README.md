# Testdata

Optional audio fixtures for examples and integration tests.

| Path | Role |
|------|------|
| [music/carrier-music.flac](music/carrier-music.flac) | ~15 s original CC0 music clip |
| [ATTRIBUTION.md](ATTRIBUTION.md) | Provenance and license for the clip |
| [licenses/CC0-1.0.txt](licenses/CC0-1.0.txt) | Verbatim CC0 deed |
| [checksums.sha256](checksums.sha256) | SHA-256 of the clipped FLAC |

**License split:** the audiostego software is GNU GPL version 2 only; the music fixture is CC0. See [ATTRIBUTION.md](ATTRIBUTION.md).

**Tests** fall back to a generated music-like carrier if this FLAC is absent (`cargo test` stays offline-friendly).

**Examples** under `examples/` require this FLAC. Regenerate with:

```bash
./scripts/clip-testdata.sh
```

Never commit stego-loaded (embedded) audio.
