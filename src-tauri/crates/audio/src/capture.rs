use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::resample::{mix_to_mono, resample_linear};
use crate::ringbuffer::AudioRing;

pub type RmsCallback = Arc<dyn Fn(f32) + Send + Sync + 'static>;

pub struct AudioCapture {
    _stream: cpal::Stream,
    pub ring: Arc<AudioRing>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("ingen input-enhet")]
    NoDevice,
    #[error("oförväntat sample format: {0:?}")]
    UnsupportedFormat(cpal::SampleFormat),
    #[error("cpal-fel: {0}")]
    Cpal(String),
}

impl AudioCapture {
    /// Skapar stream som kontinuerligt pushar INTO ringbufferen. Stream stängs
    /// när AudioCapture drop:s.
    pub fn start(ring: Arc<AudioRing>) -> Result<Self, CaptureError> {
        Self::start_with_rms(ring, None)
    }

    /// Som `start`, men `on_rms` anropas ~30 Hz med senaste buffertens RMS
    /// (clampad [0, 1]). Används för live mic-meter i UI utan att öppna en
    /// andra cpal-stream.
    pub fn start_with_rms(
        ring: Arc<AudioRing>,
        on_rms: Option<RmsCallback>,
    ) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;
        let config = device
            .default_input_config()
            .map_err(|e| CaptureError::Cpal(e.to_string()))?;

        let sample_format = config.sample_format();
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let stream_cfg = config.into();
        let err_cb = |err| tracing::error!("audio capture error: {err}");

        // Rate-limiter för rms-callback — ~30 Hz räcker för smooth UI.
        let last_emit_ns = Arc::new(AtomicU64::new(0));
        let min_interval = Duration::from_millis(33);
        let rms_cb_outer = on_rms.clone();

        let ring_cb = ring.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let rms_cb = rms_cb_outer.clone();
                let last = last_emit_ns.clone();
                device
                    .build_input_stream(
                        &stream_cfg,
                        move |data: &[f32], _| {
                            let mono = mix_to_mono(data, channels);
                            let resampled = resample_linear(&mono, sample_rate, 16000);
                            ring_cb.push_samples(&resampled);
                            if let Some(cb) = &rms_cb {
                                maybe_emit_rms(&last, min_interval, cb, rms_f32(&mono));
                            }
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| CaptureError::Cpal(e.to_string()))?
            }
            cpal::SampleFormat::I16 => {
                let norm = i16::MAX as f32;
                let rms_cb = rms_cb_outer.clone();
                let last = last_emit_ns.clone();
                device
                    .build_input_stream(
                        &stream_cfg,
                        move |data: &[i16], _| {
                            let f: Vec<f32> = data.iter().map(|&s| s as f32 / norm).collect();
                            let mono = mix_to_mono(&f, channels);
                            let resampled = resample_linear(&mono, sample_rate, 16000);
                            ring_cb.push_samples(&resampled);
                            if let Some(cb) = &rms_cb {
                                maybe_emit_rms(&last, min_interval, cb, rms_f32(&mono));
                            }
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| CaptureError::Cpal(e.to_string()))?
            }
            other => return Err(CaptureError::UnsupportedFormat(other)),
        };
        stream
            .play()
            .map_err(|e| CaptureError::Cpal(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            ring,
            sample_rate,
            channels,
        })
    }
}

fn rms_f32(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

fn maybe_emit_rms(last: &AtomicU64, min_interval: Duration, cb: &RmsCallback, rms: f32) {
    let now_ns = now_monotonic_ns();
    let last_ns = last.load(Ordering::Relaxed);
    if now_ns.saturating_sub(last_ns) < min_interval.as_nanos() as u64 {
        return;
    }
    last.store(now_ns, Ordering::Relaxed);
    cb(rms);
}

fn now_monotonic_ns() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}
