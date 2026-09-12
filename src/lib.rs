//! FFT-domain audio steganography library.

pub mod audio;
pub mod cli;
pub mod dsp;
pub mod engine;
pub mod payload;
pub mod profile;
pub mod strategy;

pub use cli::{
    ChannelMode, Cli, Commands, EccMode, LossyProfile, OutputFormat, Shaping, StrategyId,
};
pub use engine::{
    embed, extract, info, verify, EmbedReport, ExtractReport, InfoReport, VerifyReport,
};
pub use profile::ResolvedParams;
