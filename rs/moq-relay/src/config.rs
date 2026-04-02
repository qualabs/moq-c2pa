use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::{AuthConfig, ClusterConfig, UpstreamConfig, WebConfig};

/// Configuration for C2PA per-segment signing of upstream video tracks.
#[derive(clap::Args, Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct C2paConfig {
    /// Path to a C2PA manifest JSON file.
    /// When set, video tracks received from the upstream relay are signed
    /// via c2patool before being forwarded to subscribers.
    #[arg(long = "c2pa-manifest", env = "MOQ_C2PA_MANIFEST")]
    pub manifest_path: Option<std::path::PathBuf>,

    /// Path to the c2patool binary.
    #[arg(long = "c2patool-path", env = "MOQ_C2PATOOL_PATH", default_value = r"C:\Users\santi\moq-c2pa\rs\moq-c2pa\bin\c2patool.exe")]
    pub c2patool_path: std::path::PathBuf,
}

/// Top-level relay configuration, loadable from CLI arguments, environment
/// variables, or a TOML file.
#[derive(Parser, Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields, default)]
#[non_exhaustive]
pub struct Config {
	/// The QUIC/TLS configuration for the server.
	#[command(flatten)]
	#[serde(default)]
	pub server: moq_native::ServerConfig,

	/// The QUIC/TLS configuration for the client. (clustering only)
	#[command(flatten)]
	#[serde(default)]
	pub client: moq_native::ClientConfig,

	/// Log configuration.
	#[command(flatten)]
	#[serde(default)]
	pub log: moq_native::Log,

	/// Cluster configuration.
	#[command(flatten)]
	#[serde(default)]
	pub cluster: ClusterConfig,

	/// Authentication configuration.
	#[command(flatten)]
	#[serde(default)]
	pub auth: AuthConfig,

	/// Pull broadcasts from an upstream relay and serve them locally.
	#[command(flatten)]
	#[serde(default)]
	pub upstream: UpstreamConfig,

	/// C2PA signing configuration for upstream video tracks.
	#[command(flatten)]
	#[serde(default)]
	pub c2pa: C2paConfig,

	/// Optionally run a TCP HTTP/WebSocket server.
	#[command(flatten)]
	#[serde(default)]
	pub web: WebConfig,

	/// If provided, load the configuration from this file.
	#[serde(default)]
	pub file: Option<String>,

	/// Iroh specific configuration, used for both a client and server.
	#[command(flatten)]
	#[serde(default)]
	#[cfg(feature = "iroh")]
	pub iroh: moq_native::IrohEndpointConfig,
}

impl Config {
	/// Parses configuration from CLI arguments, optionally merging with a
	/// TOML file specified via `--file`. Also initializes the logger.
	pub fn load() -> anyhow::Result<Self> {
		// Parse just the CLI arguments initially.
		let mut config = Config::parse();

		// If a file is provided, load it and merge the CLI arguments.
		if let Some(file) = config.file {
			config = toml::from_str(&std::fs::read_to_string(file)?)?;
			config.update_from(std::env::args());
		}

		config.log.init();
		tracing::trace!(?config, "final config");

		Ok(config)
	}
}
