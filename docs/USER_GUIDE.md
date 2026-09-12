# audiostego user guide

Hide a short message inside an audio file and recover it later. Marks live in the short-time spectrum (STFT), not as obvious appended metadata.

This guide is for people who want to **use** the CLI. For build and test notes, see the [README](../README.md).

## Introduction

**audiostego** embeds bits into frequency bins of a carrier song, then extracts them from the loaded (stego) file. A small self-describing **capsule** header travels with the payload so extract can learn strategy, band, and related parameters. An optional **sidecar JSON** is a convenient backup of those settings.

Important distinctions:

- **Steganography ≠ encryption.** Hiding data does not keep it confidential if someone knows how to look. Use `--encrypt` with `--key` when the payload itself must stay secret; still assume a determined analyst may detect that *something* is embedded.
- **Exact recovery is realistic** on lossless float32 WAV with `qim` (default) and channel mode `mid`, with `--hop-div 1` (the reliable default when lossy is off).
- **Lossy codecs** (MP3, AAC, Opus, Vorbis) often destroy fragile marks. Treat survival as **best-effort**; measure with `verify`.

## Install

### Requirements

- Rust stable (see `rust-toolchain.toml` in the repo)
- [`ffmpeg`](https://ffmpeg.org/) on your `PATH` for non-WAV decode/encode (FLAC, MP3, Opus, AAC, Vorbis)

### Build

```bash
source "$HOME/.cargo/env"   # if using rustup and cargo is not on PATH
cd /path/to/audiostego
cargo build --release
```

The binary is `target/release/audiostego`. Put that directory on your `PATH`, or run via Cargo:

```bash
cargo run --release -- <subcommand> ...
```

Examples below use `audiostego` as if the release binary is on your `PATH`.

Global flag: `-v` / `-vv` increases logging verbosity.

## Quick start (lossless WAV)

Recommended path for exact recovery: strategy `qim`, channel `mid`, float32 WAV in and out.
The repo ships a short CC0 carrier at `testdata/music/carrier-music.flac` (see [testdata/ATTRIBUTION.md](../testdata/ATTRIBUTION.md)).

```bash
# 1. Check capacity and resolved parameters
audiostego info -i testdata/music/carrier-music.flac -s qim -c mid --message-bytes 32

# 2. Embed
audiostego embed -i testdata/music/carrier-music.flac --message-text 'hello' -o /tmp/loaded.wav \
  -s qim -c mid --sidecar /tmp/loaded.json

# 3. Extract (sidecar)
audiostego extract -i /tmp/loaded.wav -o /tmp/message.txt --sidecar /tmp/loaded.json

# 3b. Capsule-only (no sidecar) — works for lossless defaults and lossy-profile defaults
audiostego extract -i /tmp/loaded.wav -o /tmp/message-capsule.txt
```

You should get back the same text. A **sidecar** is still recommended as a backup and is **required** when you override `--band`, `--fft-size`, or `--hop-div` away from the two resolve presets. Or run the packaged script:

```bash
cargo build -q && ./examples/lossless-qim.sh
```

## Subcommands

| Command | Purpose |
|---------|---------|
| `info` | Report capacity and resolved parameters for a carrier (no embed) |
| `embed` | Write a message into a carrier → loaded audio file |
| `extract` | Recover a message from a loaded file |
| `verify` | Embed → (optional codec path) → extract; print BER / SNR-style report |

### `info`

```bash
audiostego info -i testdata/music/carrier-music.flac [common embed flags...] [--message-bytes N] [--report out.json]
```

- `-i` / `--input` — carrier file  
- `--message-bytes` — hypothetical payload size to test whether it fits  
- Shared embed parameters (`-s`, `-c`, `--lossy`, …) affect the capacity estimate  
- Prints JSON to stdout; optional `--report` writes the same to a file  

### `embed`

```bash
audiostego embed -i testdata/music/carrier-music.flac -o /tmp/loaded.wav \
  (--message-text '...' | -m message.bin) \
  [common flags...] [--sidecar side.json] [--report embed.json] [--dry-run] [--strict]
```

- Provide **either** `--message-text` **or** `-m` / `--message` (file), not both  
- `--dry-run` — resolve params and capacity only; do not write audio  
- `--strict` — refuse fragile strategy/codec combinations instead of auto-upgrading  
- `--sidecar` — write JSON with embedding parameters (recommended backup; required for non-default band/fft/hop)  
- `--report` — metrics JSON (SNR, capacity, etc.)  

### `extract`

```bash
audiostego extract -i loaded.wav -o message.bin \
  [--sidecar side.json] [--original carrier.wav] [--key ...] \
  [--max-offset 8192] [-s ...] [-c ...] [--fft-size ...] [--hop-div ...] \
  [--band LO:HI] [--strength ...] [--report out.json]
```

- `--sidecar` — restore parameters from embed (backup; required for non-preset band/fft/hop)  
- `--original` — **required** for strategy `differential`  
- `--key` — must match the key used at embed (string or `@path/to/file`)  
- `--max-offset` — how far (in samples) to search for the sync preamble (default `8192`)  
- Overrides (`-s`, `-c`, band, …) matter mainly when capsule/sidecar are missing  

### `verify`

```bash
audiostego verify -i testdata/music/carrier-music.flac (--message-text '...' | -m message.bin) \
  [common flags...] [--work-dir DIR] [--strict] [--report out.json]
```

Runs an end-to-end embed/extract cycle (and the real codec path when `--lossy` / lossy `--output-format` is set), then reports whether the message matched and the bit-error rate (BER). Use this to see whether a lossy profile survives on *your* ffmpeg build.

## Strategies (`-s` / `--strategy`)

| Value | Role | Notes |
|-------|------|--------|
| `qim` (default) | Quantization index modulation on magnitudes | Best default for **lossless** exact recovery |
| `magnitude-lsb` | Fragile magnitude LSB-style marks | Lossless only; not for lossy codecs |
| `differential` | Uses difference vs original carrier | Extract needs `--original` |
| `spread-spectrum` | Correlation-based watermark | Preferred under `--lossy`; lower effective capacity |

For MP3/AAC/Opus/Vorbis survival, use `spread-spectrum` (or let `--lossy` upgrade you there unless `--strict`).

## Channel modes (`-c` / `--channel-mode`)

| Value | Meaning |
|-------|---------|
| `mid` (default) | Mid (L+R)/2 — usually the most robust stereo choice |
| `left` / `right` | Embed in one channel |
| `channel:N` | Embed in channel index `N` (0-based) |
| `mono` | Downmix / mono work channel |
| `side` | (L−R)/2 — **destroyed by mono downmix**; prefer `mid` |
| `both-mirror` | Same payload on both channels (extract effectively uses the first work channel) |
| `both-split` | Splits payload bits across L/R (even on left, odd on right) and reassembles on extract — roughly doubles body capacity vs a single channel |

## Lossy vs lossless

Two different knobs:

| Flag | Meaning |
|------|---------|
| `--lossy <off\|mp3\|aac\|opus\|vorbis>` | Robustness **profile**: retunes FFT size, band, strength, ECC; upgrades strategy to `spread-spectrum` unless `--strict` |
| `--output-format <auto\|wav\|flac\|mp3\|opus\|aac\|vorbis>` | What **file format** is written (`auto` often follows extension / lossy profile) |
| `--bitrate` | Target bitrate in kbps for lossy encoders (default `192`) |

Practical guidance:

- **Lossless exact path:** WAV in/out, `--lossy off`, `qim` + `mid`, keep `--hop-div` at `1`.  
- **Lossy best-effort:** `--lossy mp3` (or aac/opus/vorbis), tiny messages, Reed-Solomon ECC (default under lossy profiles), then `verify` to see BER.  
- **`--strict`:** fail instead of auto-upgrading a fragile strategy into a lossy profile/format.  
- FLAC is a lossless *container*, but encode/decode via ffmpeg often uses integer PCM — do not assume bit-exact magnitude marks the way float32 WAV provides.

Exact MP3 recovery is **not** guaranteed across encoders/builds.

## Keys, ECC, and tuning

### Key and encryption

- `--key <string>` or `--key @/path/to/file` — shared secret for sync / allocation (and AEAD when encrypting)  
- `--encrypt` — ChaCha20-Poly1305 on the payload (**requires** `--key`)  
- Without `--encrypt`, the key still affects keyed sync and related processing; it is not “password protection” of the audio  

### Error correction (`--ecc`)

- `none` — no protection  
- `crc` — CRC32 integrity (typical lossless default)  
- `rs:N` — Reed-Solomon with parity `N` (1..=64); lossy profiles default toward something like `rs:16`  

### Hop, band, strength, shaping

- `--hop-div 1|2|4` — hop = `fft_size / hop_div`. Use **`1`** for reliable non-overlapping frames. `2`/`4` overlap can sound nicer but is much less reliable for recovery.  
- `--fft-size` — power of two (e.g. 2048 lossless default; lossy profiles often raise this)  
- `--band LO:HI` — frequency range in Hz (e.g. `1000:6000`)  
- `--strength` — how hard to mark (higher = more robust / more audible risk)  
- `--shaping masked|fixed` — how perturbations are shaped (default `masked`)  

When unsure, leave these unset and use `info` / `verify` rather than guessing.

## Sidecar and sync

- **Sidecar JSON** (`--sidecar` on embed; pass the same path on extract) stores parameters and related metadata so you are not solely dependent on reading the in-band capsule.
- **Capsule-only extract** (no sidecar) works when the embed used one of the two **resolve presets**: lossless defaults (`fft=2048`, `hop_div=1`, band ≈500–12000 Hz) or lossy-profile defaults (`fft=4096`, `hop_div=2`, band ≈1000–8000 Hz). Custom `--band` / `--fft-size` / `--hop-div` still need a sidecar (or matching CLI overrides).
- A **keyed sync preamble** is prepended so extract can find the payload start even with some leading delay (codec priming, accidental padding). Widen search with `--max-offset` if needed.
- Prefer keeping sidecar + key together with the loaded file for anyone who must extract later.

## Examples

Runnable scripts (require `testdata/music/carrier-music.flac`; intermediates under `$TMPDIR`):

| Script | What it shows |
|--------|----------------|
| [`examples/lossless-qim.sh`](../examples/lossless-qim.sh) | info → embed → capsule-only extract → sidecar extract |
| [`examples/encrypted.sh`](../examples/encrypted.sh) | `--encrypt --key` |
| [`examples/differential.sh`](../examples/differential.sh) | differential + `--original` |
| [`examples/lossy-mp3-verify.sh`](../examples/lossy-mp3-verify.sh) | `verify --lossy mp3` (needs ffmpeg) |

```bash
cargo build -q
./examples/lossless-qim.sh
```

## Recipes

### Lossless text in WAV

```bash
audiostego embed -i testdata/music/carrier-music.flac --message-text 'secret' -o /tmp/loaded.wav \
  -s qim -c mid --sidecar /tmp/loaded.json --key 'shared-secret'
audiostego extract -i /tmp/loaded.wav -o /tmp/out.txt --sidecar /tmp/loaded.json --key 'shared-secret'
```

### Encrypted payload (still needs a good channel)

```bash
audiostego embed -i testdata/music/carrier-music.flac --message-text 'secret' -o /tmp/loaded.wav \
  -s qim -c mid --key 'shared-secret' --encrypt --sidecar /tmp/loaded.json
audiostego extract -i /tmp/loaded.wav -o /tmp/out.txt --sidecar /tmp/loaded.json --key 'shared-secret'
```

### Differential (needs original on extract)

```bash
audiostego embed -i testdata/music/carrier-music.flac --message-text 'secret' -o /tmp/loaded.wav \
  -s differential --key 'shared-secret'
audiostego extract -i /tmp/loaded.wav --original testdata/music/carrier-music.flac \
  -o /tmp/out.txt --key 'shared-secret'
```

### Capacity check only

```bash
audiostego embed -i testdata/music/carrier-music.flac --message-text 'hello' -o /tmp/unused.wav --dry-run
# or
audiostego info -i testdata/music/carrier-music.flac --message-bytes 64
```

### MP3 best-effort + measure BER

```bash
audiostego verify -i testdata/music/carrier-music.flac --message-text 'ok' --lossy mp3 --output-format mp3
```

Or embed explicitly:

```bash
audiostego embed -i testdata/music/carrier-music.flac --message-text 'ok' -o /tmp/loaded.mp3 \
  --lossy mp3 --output-format mp3 -s spread-spectrum --bitrate 192 \
  --sidecar /tmp/loaded.json
```

Keep messages tiny and treat failure as normal unless `verify` shows `extract_ok` on your machine.

## Troubleshooting

| Symptom | Things to try |
|---------|----------------|
| Capacity / “does not fit” errors | Shorter message; longer carrier; `info --message-bytes`; lower ECC overhead; wider band / check strategy |
| CRC or decrypt failures | Wrong `--key`; missing/wrong sidecar; file re-encoded or truncated; encrypt flag mismatch |
| `ffmpeg not found` | Install ffmpeg and ensure it is on `PATH` for non-WAV formats |
| Opus sample-rate surprises | Opus is typically 48 kHz; the tool may resample using rates recorded in the capsule — keep sidecar |
| Works on WAV, fails after MP3/AAC | Expected for `qim` / LSB; use `--lossy` + `spread-spectrum` + small payload; run `verify` |
| `side` mode fails after mono mix | Prefer `mid` |
| Leading silence / delay breaks extract | Raise `--max-offset`; keep the same `--key` |
| Audible artifacts | Lower `--strength`; use `masked` shaping; shorter payload |

Re-encoding, heavy limiting, time-stretching, or aggressive normalization usually destroy extraction.

## Limits and ethics

- This is **not** a guarantee of undetectability. Spectral watermarks can leave statistical traces.  
- Do **not** treat “invisible in a casual listen” as confidentiality. Use real encryption (`--encrypt` or encrypt before embedding) and sound operational practice.  
- Lossy delivery is **best-effort**. Prove survival with `verify` on the exact toolchain you care about.  
- Prefer lawful, consented use of carrier audio. Embedding into others’ media without permission can violate copyright or platform rules. The repo fixture is CC0 — see [testdata/ATTRIBUTION.md](../testdata/ATTRIBUTION.md).

For developer tests and the `ber_sweep` helper, see the [README](../README.md).
