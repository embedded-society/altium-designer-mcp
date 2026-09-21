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

    /// Grant access to a library directory (repeatable). Adds to the config
    /// file's allowed paths, and works with no config file at all — the other
    /// settings then take their defaults
    #[arg(long = "allow", value_name = "DIR", num_args = 1..)]
    allow: Vec<PathBuf>,

    /// The Windows ANSI code page new footprint names are written in (936 for
    /// GBK, 1250, 1252, …). Overrides the config file; the default is the
    /// system's on Windows and 1252 elsewhere
    #[arg(long = "ansi-code-page", value_name = "PAGE")]
    ansi_code_page: Option<u32>,

    // Help text as strings: it names an environment variable and a URL, which
    // doc-comment lints would have wrapped in Markdown the terminal shows raw.
    #[arg(
        long = "http",
        value_name = "ADDR",
        help = "Serve the MCP Streamable HTTP transport on this address (for example \
                127.0.0.1:8080) instead of stdio, at the path /mcp. A non-loopback address \
                needs a bearer token in ALTIUM_DESIGNER_MCP_HTTP_TOKEN"
    )]
    http: Option<std::net::SocketAddr>,

    #[arg(
        long = "http-allow-origin",
        value_name = "ORIGIN",
        num_args = 1..,
        requires = "http",
        help = "A browser origin allowed to reach the HTTP transport besides the local ones, \
                for example https://claude.ai (repeatable)"
    )]
    http_allow_origin: Vec<String>,

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
    let cfg = match config::load_config_with_allow(config_path, args.allow) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            if config_path.is_none() {
                if let Some(default_path) = config::default_config_path() {
                    eprintln!("\nExpected config at: {}", default_path.display());
                    eprintln!("Create one based on config/example-config.json,");
                    eprintln!("or grant directories directly: --allow <DIR>...");
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

    // The code page new PcbLib names are written in: the one the Altium on
    // this machine reads them through, unless the user names another.
    let code_page = args
        .ansi_code_page
        .or(cfg.ansi_code_page)
        .or_else(config::system_ansi_code_page)
        .unwrap_or(1252);
    if altium_designer_mcp::altium::set_default_ansi_code_page(code_page) {
        info!(code_page, "ANSI code page for new footprint names");
    } else {
        error!(
            code_page,
            "Unsupported ANSI code page; new footprint names are written as Windows-1252"
        );
    }

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

    let rate_limiter = std::sync::Arc::new(rate_limiter);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    let result = if let Some(addr) = args.http {
        // One server per session, all drawing on the same rate limiter.
        let factory: altium_designer_mcp::mcp::http::ServerFactory =
            std::sync::Arc::new(move || {
                McpServer::new(allowed_paths.clone())
                    .with_shared_rate_limiter(std::sync::Arc::clone(&rate_limiter))
                    .with_audit_logger(audit_logger.clone())
            });
        let options = altium_designer_mcp::mcp::http::HttpOptions {
            addr,
            token: std::env::var(altium_designer_mcp::mcp::http::TOKEN_ENV).ok(),
            allowed_origins: args.http_allow_origin,
        };
        info!(%addr, token = options.token.is_some(), "MCP server ready on HTTP");
        runtime.block_on(altium_designer_mcp::mcp::http::serve(options, factory))
    } else {
        let mut server = McpServer::new(allowed_paths)
            .with_shared_rate_limiter(rate_limiter)
            .with_audit_logger(audit_logger);
        info!("MCP server ready, waiting for client connection...");
        runtime.block_on(server.run())
    };

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

    #[test]
    fn quiet_forces_error_regardless_of_verbose_or_config() {
        assert_eq!(get_log_level(0, true, "trace"), Level::ERROR);
        assert_eq!(get_log_level(3, true, "info"), Level::ERROR);
    }

    #[test]
    fn verbose_flags_map_to_levels() {
        assert_eq!(get_log_level(1, false, "warn"), Level::INFO);
        assert_eq!(get_log_level(2, false, "warn"), Level::DEBUG);
        assert_eq!(get_log_level(3, false, "warn"), Level::TRACE);
        // Beyond -vvv still saturates at TRACE.
        assert_eq!(get_log_level(9, false, "warn"), Level::TRACE);
    }

    #[test]
    fn config_level_used_when_no_verbose_flags() {
        assert_eq!(get_log_level(0, false, "trace"), Level::TRACE);
        assert_eq!(get_log_level(0, false, "debug"), Level::DEBUG);
        assert_eq!(get_log_level(0, false, "info"), Level::INFO);
        assert_eq!(get_log_level(0, false, "warn"), Level::WARN);
        assert_eq!(get_log_level(0, false, "error"), Level::ERROR);
        // Case-insensitive.
        assert_eq!(get_log_level(0, false, "INFO"), Level::INFO);
        // Unknown config level falls back to WARN.
        assert_eq!(get_log_level(0, false, "chatty"), Level::WARN);
    }
}
