use audiostego::cli::{Cli, Commands};
use audiostego::{embed, extract, info, verify};
use clap::Parser;
use tracing_subscriber::EnvFilter;

fn main() {
    let cli = Cli::parse();
    let filter = match cli.verbose {
        0 => "audiostego=info",
        1 => "audiostego=debug",
        _ => "audiostego=trace",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let result = match cli.command {
        Commands::Embed(args) => {
            let out_hint = if args.dry_run {
                "(dry-run)".to_string()
            } else {
                args.output.display().to_string()
            };
            embed(&args).map(|r| {
                eprintln!(
                    "embedded {} bytes ({} bits used / {} capacity), SNR={:.1} dB -> {}",
                    r.message_bytes, r.used_bits, r.capacity_bits, r.snr_db, out_hint
                );
                if !r.upgraded.is_empty() {
                    for u in &r.upgraded {
                        eprintln!("note: {u}");
                    }
                }
            })
        }
        Commands::Extract(args) => {
            let out = args.output.display().to_string();
            extract(&args).map(|(msg, r)| {
                eprintln!(
                    "recovered {} bytes (strategy={}, sync_offset={}, score={:.3}) -> {}",
                    msg.len(),
                    r.strategy,
                    r.sync_offset,
                    r.sync_score,
                    out
                );
            })
        }
        Commands::Verify(args) => verify(&args).map(|r| {
            eprintln!(
                "verify: ok={} BER={:.6} recovered={}/{}",
                r.extract_ok, r.ber, r.recovered_bytes, r.expected_bytes
            );
        }),
        Commands::Info(args) => info(&args).map(|_| ()),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
