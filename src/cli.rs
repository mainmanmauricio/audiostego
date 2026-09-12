//! Command-line interface definitions.

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "audiostego",
    about = "FFT-domain audio steganography: embed and recover messages in songs",
    version,
    long_version = concat!(
        env!("CARGO_PKG_VERSION"),
        "\nCopyright (c) 2024–2026 Maurice Gittens",
        "\nLicense: GNU GPL version 2 only"
    ),
    after_help = "Copyright (c) 2024–2026 Maurice Gittens\nLicense: GNU GPL version 2 only"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Increase logging verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Embed a message into a carrier song
    Embed(EmbedArgs),
    /// Extract a message from a loaded song
    Extract(ExtractArgs),
    /// Embed, optionally transcode, extract, and report BER / SNR
    Verify(VerifyArgs),
    /// Report capacity and resolved parameters for a carrier
    Info(InfoArgs),
}

#[derive(Debug, Clone, Parser)]
pub struct EmbedArgs {
    /// Carrier song (wav/flac/mp3/aac/ogg)
    #[arg(short = 'i', long)]
    pub input: PathBuf,

    /// Message file to embed
    #[arg(short = 'm', long, conflicts_with = "message_text")]
    pub message: Option<PathBuf>,

    /// Literal message text to embed
    #[arg(long, conflicts_with = "message")]
    pub message_text: Option<String>,

    /// Output loaded audio file
    #[arg(short = 'o', long)]
    pub output: PathBuf,

    #[command(flatten)]
    pub common: CommonEmbedParams,

    /// Write sidecar JSON with embedding parameters
    #[arg(long)]
    pub sidecar: Option<PathBuf>,

    /// Write metrics/report JSON
    #[arg(long)]
    pub report: Option<PathBuf>,

    /// Compute capacity and resolve params without writing audio
    #[arg(long)]
    pub dry_run: bool,

    /// Refuse fragile strategy/codec combinations instead of upgrading
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, Clone, Parser)]
pub struct ExtractArgs {
    /// Loaded (stego) audio file
    #[arg(short = 'i', long)]
    pub input: PathBuf,

    /// Original carrier (required for differential strategy)
    #[arg(long)]
    pub original: Option<PathBuf>,

    /// Output recovered message file
    #[arg(short = 'o', long)]
    pub output: PathBuf,

    /// Sidecar JSON restoring embedding parameters
    #[arg(long)]
    pub sidecar: Option<PathBuf>,

    /// Maximum sample offset to search for sync
    #[arg(long, default_value_t = 8192)]
    pub max_offset: usize,

    /// Shared secret key (string or @file)
    #[arg(long)]
    pub key: Option<String>,

    /// Override strategy if no capsule/sidecar
    #[arg(short = 's', long)]
    pub strategy: Option<StrategyId>,

    /// Override channel mode
    #[arg(short = 'c', long, value_parser = parse_channel_mode)]
    pub channel_mode: Option<ChannelMode>,

    /// FFT size override
    #[arg(long)]
    pub fft_size: Option<usize>,

    /// Hop divisor override (1, 2, or 4)
    #[arg(long)]
    pub hop_div: Option<u32>,

    /// Band override as LO:HI Hz
    #[arg(long)]
    pub band: Option<String>,

    /// Strength override
    #[arg(long)]
    pub strength: Option<f32>,

    /// Write metrics/report JSON
    #[arg(long)]
    pub report: Option<PathBuf>,
}

#[derive(Debug, Clone, Parser)]
pub struct VerifyArgs {
    #[arg(short = 'i', long)]
    pub input: PathBuf,

    #[arg(short = 'm', long, conflicts_with = "message_text")]
    pub message: Option<PathBuf>,

    #[arg(long, conflicts_with = "message")]
    pub message_text: Option<String>,

    /// Working directory for intermediate files (default: temp)
    #[arg(long)]
    pub work_dir: Option<PathBuf>,

    #[command(flatten)]
    pub common: CommonEmbedParams,

    #[arg(long)]
    pub strict: bool,

    #[arg(long)]
    pub report: Option<PathBuf>,
}

#[derive(Debug, Clone, Parser)]
pub struct InfoArgs {
    #[arg(short = 'i', long)]
    pub input: PathBuf,

    #[command(flatten)]
    pub common: CommonEmbedParams,

    /// Hypothetical message size in bytes (for capacity check)
    #[arg(long)]
    pub message_bytes: Option<usize>,

    #[arg(long)]
    pub report: Option<PathBuf>,
}

/// Shared embedding / profile parameters.
#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
pub struct CommonEmbedParams {
    /// Embedding strategy
    #[arg(short = 's', long, default_value = "qim")]
    pub strategy: StrategyId,

    /// Stereo / channel handling (mono|left|right|channel:N|mid|side|both-mirror|both-split)
    #[arg(short = 'c', long, default_value = "mid", value_parser = parse_channel_mode)]
    pub channel_mode: ChannelMode,

    /// Lossy robustness profile (retunes defaults)
    #[arg(long, default_value = "off")]
    pub lossy: LossyProfile,

    /// Output container/codec
    #[arg(long, default_value = "auto")]
    pub output_format: OutputFormat,

    /// Target bitrate for lossy encoders (kbps)
    #[arg(long, default_value_t = 192)]
    pub bitrate: u32,

    /// FFT size (power of two)
    #[arg(long)]
    pub fft_size: Option<usize>,

    /// Hop = fft_size / hop_div (1, 2, or 4). Use 1 for reliable non-overlapping embeds.
    #[arg(long)]
    pub hop_div: Option<u32>,

    /// Frequency band as LO:HI in Hz (e.g. 1000:8000)
    #[arg(long)]
    pub band: Option<String>,

    /// Embedding strength
    #[arg(long)]
    pub strength: Option<f32>,

    /// Perturbation shaping
    #[arg(long, default_value = "masked")]
    pub shaping: Shaping,

    /// Error-correction mode: none | crc | rs:N
    #[arg(long)]
    pub ecc: Option<String>,

    /// Shared secret key (string or @file)
    #[arg(long)]
    pub key: Option<String>,

    /// Encrypt payload with ChaCha20-Poly1305 (requires --key)
    #[arg(long, default_value_t = false)]
    pub encrypt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyId {
    Differential,
    MagnitudeLsb,
    Qim,
    SpreadSpectrum,
}

impl StrategyId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Differential => "differential",
            Self::MagnitudeLsb => "magnitude-lsb",
            Self::Qim => "qim",
            Self::SpreadSpectrum => "spread-spectrum",
        }
    }

    pub fn requires_original(self) -> bool {
        matches!(self, Self::Differential)
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Differential),
            1 => Some(Self::MagnitudeLsb),
            2 => Some(Self::Qim),
            3 => Some(Self::SpreadSpectrum),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Differential => 0,
            Self::MagnitudeLsb => 1,
            Self::Qim => 2,
            Self::SpreadSpectrum => 3,
        }
    }
}

impl std::fmt::Display for StrategyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelMode {
    Mono,
    Left,
    Right,
    Channel(u16),
    Mid,
    Side,
    BothMirror,
    BothSplit,
}

impl ChannelMode {
    pub fn as_str(&self) -> String {
        match self {
            Self::Mono => "mono".into(),
            Self::Left => "left".into(),
            Self::Right => "right".into(),
            Self::Channel(n) => format!("channel:{n}"),
            Self::Mid => "mid".into(),
            Self::Side => "side".into(),
            Self::BothMirror => "both-mirror".into(),
            Self::BothSplit => "both-split".into(),
        }
    }

    pub fn warns_mono_fragile(&self) -> bool {
        matches!(self, Self::Side)
    }

    pub fn to_code(&self) -> u8 {
        match self {
            Self::Mono => 0,
            Self::Left => 1,
            Self::Right => 2,
            Self::Channel(n) => 3 + (*n).min(12) as u8,
            Self::Mid => 20,
            Self::Side => 21,
            Self::BothMirror => 22,
            Self::BothSplit => 23,
        }
    }

    pub fn from_code(c: u8) -> Option<Self> {
        match c {
            0 => Some(Self::Mono),
            1 => Some(Self::Left),
            2 => Some(Self::Right),
            3..=15 => Some(Self::Channel((c - 3) as u16)),
            20 => Some(Self::Mid),
            21 => Some(Self::Side),
            22 => Some(Self::BothMirror),
            23 => Some(Self::BothSplit),
            _ => None,
        }
    }
}

fn parse_channel_mode(s: &str) -> Result<ChannelMode, String> {
    s.parse()
}

impl std::str::FromStr for ChannelMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        Ok(match lower.as_str() {
            "mono" => Self::Mono,
            "left" => Self::Left,
            "right" => Self::Right,
            "mid" => Self::Mid,
            "side" => Self::Side,
            "both-mirror" | "both_mirror" => Self::BothMirror,
            "both-split" | "both_split" => Self::BothSplit,
            other if other.starts_with("channel:") => {
                let n: u16 = other[8..]
                    .parse()
                    .map_err(|_| format!("invalid channel index in '{s}'"))?;
                Self::Channel(n)
            }
            _ => return Err(format!("unknown channel mode '{s}'")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LossyProfile {
    Off,
    Mp3,
    Aac,
    Opus,
    Vorbis,
}

impl LossyProfile {
    pub fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Mp3 => "mp3",
            Self::Aac => "aac",
            Self::Opus => "opus",
            Self::Vorbis => "vorbis",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Auto,
    Wav,
    Flac,
    Mp3,
    Opus,
    Aac,
    Vorbis,
}

impl OutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Aac => "aac",
            Self::Vorbis => "vorbis",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Auto | Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Aac => "aac",
            Self::Vorbis => "ogg",
        }
    }

    pub fn is_lossy(self) -> bool {
        matches!(self, Self::Mp3 | Self::Opus | Self::Aac | Self::Vorbis)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shaping {
    Fixed,
    Masked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EccMode {
    None,
    Crc,
    ReedSolomon { parity: u8 },
}

impl EccMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        let lower = s.to_ascii_lowercase();
        if lower == "none" {
            return Ok(Self::None);
        }
        if lower == "crc" {
            return Ok(Self::Crc);
        }
        if let Some(rest) = lower.strip_prefix("rs:") {
            let n: u8 = rest
                .parse()
                .map_err(|_| format!("invalid reed-solomon parity in '{s}'"))?;
            if n == 0 || n > 64 {
                return Err("reed-solomon parity must be 1..=64".into());
            }
            return Ok(Self::ReedSolomon { parity: n });
        }
        Err(format!("unknown ecc mode '{s}' (expected none|crc|rs:N)"))
    }

    pub fn as_str(self) -> String {
        match self {
            Self::None => "none".into(),
            Self::Crc => "crc".into(),
            Self::ReedSolomon { parity } => format!("rs:{parity}"),
        }
    }

    pub fn to_code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Crc => 1,
            Self::ReedSolomon { parity } => 2 + parity.min(64),
        }
    }

    pub fn from_code(c: u8) -> Option<Self> {
        match c {
            0 => Some(Self::None),
            1 => Some(Self::Crc),
            2..=66 => Some(Self::ReedSolomon { parity: c - 2 }),
            _ => None,
        }
    }
}
