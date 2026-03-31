use anyhow::Context;
use moq_lite::OriginProducer;
use url::Url;

/// Configuration for subscribing to a remote MoQ relay and republishing locally.
#[derive(clap::Args, Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct UpstreamConfig {
	/// Connect to this MoQ relay URL and pull all broadcasts for local subscribers.
	#[arg(long = "upstream-url", env = "MOQ_UPSTREAM_URL")]
	pub url: Option<Url>,

	/// JWT token for authenticating with the upstream relay.
	#[arg(long = "upstream-token", env = "MOQ_UPSTREAM_TOKEN")]
	pub token: Option<String>,
}

/// Subscribes to a remote MoQ relay and makes its broadcasts available locally.
///
/// Broadcasts received from the upstream are placed in the cluster's `secondary`
/// origin, which gets merged into `combined` for local subscribers to consume.
pub struct Upstream {
	config: UpstreamConfig,
	client: moq_native::Client,
	/// Receives broadcasts from the remote relay.
	secondary: OriginProducer,
}

impl Upstream {
	/// Creates a new upstream subscriber.
	pub fn new(config: UpstreamConfig, client: moq_native::Client, secondary: OriginProducer) -> Self {
		Self {
			config,
			client,
			secondary,
		}
	}

	/// Runs the upstream connection loop.
	///
	/// If no URL is configured, returns immediately (removing this branch from
	/// the caller's `tokio::select!`). Otherwise reconnects with exponential
	/// backoff on failure and never returns `Ok`.
	pub async fn run(self) -> anyhow::Result<()> {
		let Some(ref base_url) = self.config.url else {
			return Ok(());
		};

		let mut url = base_url.clone();
		if let Some(token) = &self.config.token {
			url.query_pairs_mut().append_pair("jwt", token);
		}

		let mut backoff = 1u64;

		loop {
			match self.connect_once(&url).await {
				Ok(()) => {
					tracing::info!("upstream connection closed");
					backoff = 1;
				}
				Err(err) => {
					tracing::warn!(%err, backoff, "upstream error, reconnecting");
					backoff = (backoff * 2).min(300);
				}
			}

			tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
		}
	}

	#[tracing::instrument("upstream", skip_all, err)]
	async fn connect_once(&self, url: &Url) -> anyhow::Result<()> {
		let mut log_url = url.clone();
		log_url.set_query(None);
		tracing::info!(%log_url, "connecting");

		let session = self
			.client
			.clone()
			.with_consume(self.secondary.clone())
			.connect(url.clone())
			.await
			.context("failed to connect to upstream")?;

		session.closed().await.map_err(Into::into)
	}
}
