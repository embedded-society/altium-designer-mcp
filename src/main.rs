//! altium-designer-mcp: MCP server for AI-assisted Altium Designer library management
//!
//! This tool provides file I/O and primitive placement capabilities that enable
//! AI assistants to create and manage Altium Designer component libraries.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tracing::{error, info, Level};

use altium_designer_mcp::config;
use altium_designer_mcp::mcp::server::McpServer;
use altium_designer_mcp::security::{AuditLogger, RateLimiter};

/// MCP server for AI-assisted Altium Designer library management.
///
/// Provides file I/O and primitive placement tools that enable AI assistants
/// to create and manage Altium Designer component libraries.
#[derive(Parser, Debug)]
#[command(name = "altium-designer-mcp")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(value_name = "CONFIG_FILE")]
    config: Option<PathBuf>,

    /// Increase logging verbosity (-v for info, -vv for debug, -vvv for trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Decrease logging verbosity (only show errors)
    #[arg(short, long)]
    quiet: bool,
}

/// Determines the log level from CLI arguments.
#[allow(clippy::match_same_arms)] // Explicit "warn" arm for clarity
fn get_log_level(verbose: u8, quiet: bool, config_level: &str) -> Level {
    if quiet {
        return Level::ERROR;
    }

    match verbose {
        0 => match config_level.to_lowercase().as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::WARN, // Default to warn for unknown levels
        },
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    }
}

/// Initialises the tracing subscriber for logging.
///
/// The level derived from `-v`/`-q`/`config.logging.level` becomes the
/// default directive, while the `RUST_LOG` environment variable can override
/// verbosity per module (e.g.
/// `RUST_LOG=altium_designer_mcp::altium::pcblib::reader=trace`). This is
/// invaluable when debugging the binary OLE read/write paths without
/// recompiling.
fn init_tracing(level: Level) {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}

/// Entry point for the altium-designer-mcp server.
fn main() -> ExitCode {
    let args = Args::parse();

    // Load configuration
    let config_path = args.config.as_deref();
    let cfg = match config::load_config(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            if config_path.is_none() {
                if let Some(default_path) = config::default_config_path() {
                    eprintln!("\nExpected config at: {}", default_path.display());
                    eprintln!("Create one based on config/example-config.json");
                }
            }
            return ExitCode::FAILURE;
        }
    };

    // Initialise logging
    let log_level = get_log_level(args.verbose, args.quiet, &cfg.logging.level);
    init_tracing(log_level);

    // Display GPL license notice (required by GPLv3 Section 5d)
    eprintln!(
        "altium-designer-mcp {}  Copyright (C) 2026  The Embedded Society",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!("This program comes with ABSOLUTELY NO WARRANTY.");
    eprintln!("This is free software, licensed under GPL-3.0-or-later.");
    eprintln!("Source: {}", env!("CARGO_PKG_REPOSITORY"));
    eprintln!();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting altium-designer-mcp server"
    );

    // Get allowed paths from config
    let allowed_paths = if cfg.allowed_paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        cfg.allowed_paths
    };

    info!(
        allowed_paths = ?allowed_paths,
        "Allowed paths configured"
    );

    // Create MCP server with a rate limiter for destructive operations,
    // configured from the user's settings.
    let rate_limiter = RateLimiter::new(cfg.rate_limit.max_burst, cfg.rate_limit.refill_per_sec);
    info!(
        max_burst = cfg.rate_limit.max_burst,
        refill_per_sec = cfg.rate_limit.refill_per_sec,
        "Rate limiting destructive operations"
    );

    let audit_logger = cfg.logging.audit_log_path.clone().map(AuditLogger::new);
    if let Some(path) = &cfg.logging.audit_log_path {
        info!(audit_log = %path.display(), "Audit logging destructive operations");
    }

    let mut server = McpServer::new(allowed_paths)
        .with_rate_limiter(rate_limiter)
        .with_audit_logger(audit_logger);

    info!("MCP server ready, waiting for client connection...");

    // Run the server
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    let result = runtime.block_on(server.run());

    match result {
        Ok(()) => {
            info!("Server shut down gracefully");
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!(error = %e, "Server error");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
