# audiostego

FFT-domain audio steganography CLI written in Rust. Embed a message into a song's short-time spectrum and recover it later.

**User guide:** [docs/USER_GUIDE.md](docs/USER_GUIDE.md)

## Features

- Strategies: `differential`, `magnitude-lsb`, `qim` (default), `spread-spectrum`
- Channel modes: `mono`, `left`, `right`, `channel:N`, `mid` (default), `side`, `both-mirror`, `both-split`
- Lossy robustness profiles (`--lossy`) and output formats (`--output-format`) via ffmpeg for non-WAV
- Self-describing capsule header, optional sidecar JSON, keyed sync preamble
- CRC / Reed-Solomon ECC; optional ChaCha20-Poly1305 (`--encrypt`)
- Subcommands: `embed`, `extract`, `verify`, `info`

## Requirements

- Rust stable (see `rust-toolchain.toml`)
- `ffmpeg` on `PATH` for non-WAV decode/encode (mp3/opus/aac/flac/vorbis)

## Build

```bash
source "$HOME/.cargo/env"   # if using rustup
cargo build --release
```

Binary: `target/release/audiostego`. See the [user guide](docs/USER_GUIDE.md) for install, recipes, and troubleshooting.

## Packaging

Build local `.deb` and `.rpm` packages (ships `/usr/bin/audiostego` plus docs; depends on `ffmpeg`; does not include `ber_sweep`):

```bash
cargo install cargo-deb cargo-generate-rpm
cargo build --release
cargo deb
cargo generate-rpm
```

Artifacts:

- Debian: `target/debian/audiostego_*.deb`
- RPM: `target/generate-rpm/audiostego-*.rpm`

Install examples:

```bash
sudo apt install ./target/debian/audiostego_*.deb
sudo dnf install ./target/generate-rpm/audiostego-*.rpm
# or: sudo rpm -i ./target/generate-rpm/audiostego-*.rpm
```

## Examples

```bash
cargo build -q
./examples/lossless-qim.sh      # capsule-only + sidecar extract
./examples/encrypted.sh
./examples/differential.sh
./examples/lossy-mp3-verify.sh  # needs ffmpeg
```

Scripts require [`testdata/music/carrier-music.flac`](testdata/music/carrier-music.flac) (CC0 clip; regenerate with `./scripts/clip-testdata.sh`). See the [user guide](docs/USER_GUIDE.md).

## Tests

```bash
cargo test
cargo test --test lossy_roundtrip   # codec smokes need ffmpeg
AUDIOSTEGO_LOSSY_STRICT=1 cargo test --test lossy_roundtrip  # assert MP3 exact recovery
cargo run --release --bin ber_sweep
```

Integration suites live under `tests/` (shared fixtures in `tests/common/`). Music tests use the vendored FLAC when present, otherwise a generated music-like carrier. Exact MP3 recovery is not guaranteed without `AUDIOSTEGO_LOSSY_STRICT=1`.

## Testdata

Short original CC0 music clip for examples and optional music round-trips: [testdata/](testdata/). Software is GNU GPL version 2 only; the audio fixture is CC0 — see [testdata/ATTRIBUTION.md](testdata/ATTRIBUTION.md).

## License

Copyright (c) 2024–2026 Maurice Gittens. Released under GNU GPL version 2 only — see [LICENSE](LICENSE).
