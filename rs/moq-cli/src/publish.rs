use clap::Subcommand;
use hang::moq_lite;
use moq_mux::import;

#[derive(Subcommand, Clone)]
pub enum PublishFormat {
	Avc3,
	Fmp4 {
		/// Transmit the fMP4 container directly instead of decoding it.
		#[arg(long)]
		passthrough: bool,

		#[cfg(feature = "c2pa")]
		#[arg(long)]
		c2pa_manifest: Option<std::path::PathBuf>,

		#[cfg(feature = "c2pa")]
		#[arg(long, default_value = r"C:\Users\santi\moq-c2pa\rs\moq-c2pa\bin\c2patool.exe")]
		c2patool_path: std::path::PathBuf,
	},
	// NOTE: No aac support because it needs framing.
	Hls {
		/// URL or file path of an HLS playlist to ingest.
		#[arg(long)]
		playlist: String,

		/// Transmit the fMP4 segments directly instead of decoding them.
		#[arg(long)]
		passthrough: bool,
	},
}

enum PublishDecoder {
	Avc3(Box<import::Avc3>),
	Fmp4(Box<import::Fmp4>),
	Hls(Box<import::Hls>),
}

pub struct Publish {
	decoder: PublishDecoder,
	broadcast: moq_lite::BroadcastProducer,
}

impl Publish {
	pub fn new(format: &PublishFormat) -> anyhow::Result<Self> {
		let mut broadcast = moq_lite::BroadcastProducer::default();
		let catalog = moq_mux::CatalogProducer::new(&mut broadcast)?;

		let decoder = match format {
			PublishFormat::Avc3 => {
				let avc3 = import::Avc3::new(broadcast.clone(), catalog.clone());
				PublishDecoder::Avc3(Box::new(avc3))
			}
			PublishFormat::Fmp4 {
				passthrough,
				#[cfg(feature = "c2pa")]
				c2pa_manifest,
				#[cfg(feature = "c2pa")]
				c2patool_path,
			} => {
				#[cfg(feature = "c2pa")]
				let signer = match (c2pa_manifest, passthrough) {
					(Some(_), false) => anyhow::bail!("--c2pa-manifest requires --passthrough"),
					(Some(manifest), true) => Some(std::sync::Arc::new(moq_c2pa::SegmentSigner {
						manifest_path: manifest.clone(),
						c2patool_path: c2patool_path.clone(),
					})),
					(None, _) => None,
				};

				let fmp4 = import::Fmp4::new(
					broadcast.clone(),
					catalog.clone(),
					import::Fmp4Config {
						passthrough: *passthrough,
						#[cfg(feature = "c2pa")]
						signer,
					},
				);
				PublishDecoder::Fmp4(Box::new(fmp4))
			}
			PublishFormat::Hls { playlist, passthrough } => {
				let hls = import::Hls::new(
					broadcast.clone(),
					catalog.clone(),
					import::HlsConfig {
						playlist: playlist.clone(),
						client: None,
						passthrough: *passthrough,
					},
				)?;
				PublishDecoder::Hls(Box::new(hls))
			}
		};

		Ok(Self { decoder, broadcast })
	}

	pub fn consume(&self) -> moq_lite::BroadcastConsumer {
		self.broadcast.consume()
	}
}

impl Publish {
	pub async fn run(mut self) -> anyhow::Result<()> {
		let mut stdin = tokio::io::stdin();

		match &mut self.decoder {
			PublishDecoder::Avc3(decoder) => decoder.decode_from(&mut stdin).await,
			PublishDecoder::Fmp4(decoder) => decoder.decode_from(&mut stdin).await,
			PublishDecoder::Hls(decoder) => decoder.run().await,
		}
	}
}
