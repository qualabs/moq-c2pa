use std::sync::Arc;

use bytes::Bytes;
use moq_lite::{BroadcastConsumer, BroadcastProducer, OriginConsumer, OriginProducer};
use moq_c2pa::SegmentSigner;

use crate::C2paConfig;

/// Watches an origin for announced broadcasts and republishes them with
/// video tracks signed via c2patool. Whether each track is video is determined
/// dynamically by attempting to sign the first frame — no hang catalog required.
/// Audio, catalog, and other non-video tracks fall through unsigned.
pub struct C2paProxy {
	source: OriginConsumer,
	dest: OriginProducer,
	signer: Arc<SegmentSigner>,
}

impl C2paProxy {
	/// Create a new proxy. Returns `None` if no manifest path is configured.
	pub fn new(config: &C2paConfig, source: OriginConsumer, dest: OriginProducer) -> Option<Self> {
		let manifest = config.manifest_path.clone()?;
		Some(Self {
			source,
			dest,
			signer: Arc::new(SegmentSigner {
				manifest_path: manifest,
				c2patool_path: config.c2patool_path.clone(),
			}),
		})
	}

	/// Run the proxy loop. Spawns a signing task for each announced broadcast.
	pub async fn run(mut self) -> anyhow::Result<()> {
		loop {
			let Some((path, broadcast)) = self.source.announced().await else {
				return Ok(());
			};
			if let Some(broadcast) = broadcast {
				let dest = self.dest.clone();
				let signer = self.signer.clone();
				tokio::spawn(async move {
					if let Err(e) = sign_broadcast(broadcast, dest, path, signer).await {
						tracing::warn!(%e, "c2pa broadcast signing error");
					}
				});
			}
		}
	}
}

/// Republish a broadcast through a dynamic signing proxy.
///
/// Creates a dynamic output broadcast and handles each track request on demand.
/// For each requested track, the first frame is used to probe whether c2patool
/// can sign it. If signing succeeds the track is signed for its lifetime;
/// otherwise it passes through unchanged.
async fn sign_broadcast(
	source: BroadcastConsumer,
	dest: OriginProducer,
	path: moq_lite::PathOwned,
	signer: Arc<SegmentSigner>,
) -> anyhow::Result<()> {
	let output = BroadcastProducer::new();
	let mut dynamic = output.dynamic();

	dest.publish_broadcast(&path, output.consume());

	loop {
		tokio::select! {
			_ = source.closed() => return Ok(()),
			result = dynamic.requested_track() => {
				let tp = match result {
					Ok(tp) => tp,
					Err(_) => return Ok(()),
				};

				let tc = match source.subscribe_track(&tp.info) {
					Ok(tc) => tc,
					Err(e) => {
						tracing::warn!(%path, track = %tp.info.name, %e, "c2pa: failed to subscribe to source track");
						continue;
					}
				};

				let signer = signer.clone();
				tokio::spawn(async move {
					if let Err(e) = sign_track_adaptive(tc, tp, signer).await {
						tracing::warn!(%e, "c2pa: track error");
					}
				});
			}
		}
	}
}

/// Process a single track, probing the first frame to decide sign vs passthrough.
///
/// Passes the full CMAF segment directly to c2patool. If signing succeeds the
/// whole track is signed (CMAF video); if it fails the track is forwarded as-is
/// (audio, catalog, etc.) without spawning additional c2patool processes.
async fn sign_track_adaptive(
	mut consumer: moq_lite::TrackConsumer,
	mut producer: moq_lite::TrackProducer,
	signer: Arc<SegmentSigner>,
) -> anyhow::Result<()> {
	// Grab the first group and its first frame for the probe.
	let Some(mut first_group_c) = consumer.next_group().await? else {
		producer.finish()?;
		return Ok(());
	};
	let Some(first_frame) = first_group_c.read_frame().await? else {
		let mut gp = producer.append_group()?;
		gp.finish()?;
		producer.finish()?;
		return Ok(());
	};

	// Probe: attempt to sign the full CMAF segment. c2patool handles sidx/styp
	// prefixes natively. Success means this is a signable video track.
	let signer2 = signer.clone();
	let probe_frame = first_frame.clone();
	let probe = tokio::task::spawn_blocking(move || signer2.sign(&probe_frame)).await?;
	let do_sign = probe.is_ok();

	if do_sign {
		tracing::info!(track = %producer.info.name, "c2pa: signing CMAF video track");
	} else {
		tracing::debug!(track = %producer.info.name, "c2pa: non-video track, passing through unsigned");
	}

	// Write first frame (signed or original).
	let mut first_gp = producer.append_group()?;
	if do_sign {
		first_gp.write_frame(probe.unwrap())?; // safe: do_sign guarantees Ok
	} else {
		first_gp.write_frame(first_frame)?;
	}

	// Finish first group.
	while let Some(frame) = first_group_c.read_frame().await? {
		if do_sign {
			first_gp.write_frame(sign_frame(frame, &signer).await)?;
		} else {
			first_gp.write_frame(frame)?;
		}
	}
	first_gp.finish()?;

	// Remaining groups.
	while let Some(mut group_c) = consumer.next_group().await? {
		let mut group_p = producer.append_group()?;
		while let Some(frame) = group_c.read_frame().await? {
			if do_sign {
				group_p.write_frame(sign_frame(frame, &signer).await)?;
			} else {
				group_p.write_frame(frame)?;
			}
		}
		group_p.finish()?;
	}

	producer.finish()?;
	Ok(())
}
// to-do change fram to chunk in repo
/// Sign a single CMAF frame, returning the signed bytes or the original on error.
async fn sign_frame(frame: Bytes, signer: &Arc<SegmentSigner>) -> Bytes {
	let signer2 = signer.clone();
	let original = frame.clone();
	match tokio::task::spawn_blocking(move || signer2.sign(&frame)).await {
		Ok(Ok(signed)) => signed,
		Ok(Err(e)) => {
			tracing::warn!(%e, "c2pa: signing failed, passing through original");
			original
		}
		Err(e) => {
			tracing::warn!(%e, "c2pa: spawn_blocking join error, passing through original");
			original
		}
	}
}
