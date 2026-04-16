//! Minimal volym-mätare via cpal. Öppnar default input-device, beräknar RMS
//! per audio-callback och rate-limitar anropen till `on_volume` till ~30 Hz så
//! UI-eventen inte översvämmas.
//!
//! Används av walking skeleton för att ge användaren visuell feedback medan
//! PTT hålls. Full WASAPI + ringbuffer + VAD-pipeline (för STT) byggs i iter 2.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug, thiserror::Error)]
pub enum VolumeMeterError {
    #[error("ingen default input-enhet tillgänglig")]
    NoInputDevice,
    #[error("kunde inte hämta input-config: {0}")]
    ConfigError(String),
    #[error("kunde inte bygga audio-stream: {0}")]
    BuildStreamError(String),
    #[error("kunde inte starta audio-stream: {0}")]
    PlayStreamError(String),
}

/// Öppnar default input-mic och kör `on_volume(rms)` ~30 gånger per sekund
/// medan VolumeMeter lever. Drop av instansen stoppar streamen.
pub struct VolumeMeter {
    _stream: cpal::Stream,
}

impl VolumeMeter {
    pub fn start<F>(on_volume: F) -> Result<Self, VolumeMeterError>
    where
        F: Fn(f32) + Send + Sync + 'static,
    {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(VolumeMeterError::NoInputDevice)?;

        let config = device
            .default_input_config()
            .map_err(|e| VolumeMeterError::ConfigError(e.to_string()))?;

        tracing::info!(
            "audio: input '{}' @ {} Hz, {} ch, {:?}",
            device.name().unwrap_or_else(|_| "okänd".into()),
            config.sample_rate().0,
            config.channels(),
            config.sample_format()
        );

        let sample_format = config.sample_format();
        let stream_config = config.into();

        // Rate-limiter: sparar tid för senaste emit i nanosekunder sen epoch.
        let last_emit_ns = Arc::new(AtomicU64::new(0));
        let callback: Arc<dyn Fn(f32) + Send + Sync + 'static> = Arc::new(on_volume);
        let min_interval = Duration::from_millis(33); // ~30 Hz

        let err_cb = |err| tracing::error!("audio stream fel: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let cb = callback.clone();
                let last = last_emit_ns.clone();
                let call_counter = Arc::new(AtomicU64::new(0));
                let cc = call_counter.clone();
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[f32], _| {
                            let n = cc.fetch_add(1, Ordering::Relaxed);
                            let rms = rms_f32(data);
                            if n % 50 == 0 {
                                tracing::debug!(
                                    "audio callback #{}: {} samples, rms={:.5}",
                                    n,
                                    data.len(),
                                    rms
                                );
                            }
                            maybe_emit(&last, min_interval, &cb, rms);
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| VolumeMeterError::BuildStreamError(e.to_string()))?
            }
            cpal::SampleFormat::I16 => {
                let cb = callback.clone();
                let last = last_emit_ns.clone();
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[i16], _| {
                            let rms = rms_i16(data);
                            maybe_emit(&last, min_interval, &cb, rms);
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| VolumeMeterError::BuildStreamError(e.to_string()))?
            }
            cpal::SampleFormat::U16 => {
                let cb = callback.clone();
                let last = last_emit_ns.clone();
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[u16], _| {
                            let rms = rms_u16(data);
                            maybe_emit(&last, min_interval, &cb, rms);
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| VolumeMeterError::BuildStreamError(e.to_string()))?
            }
            other => {
                return Err(VolumeMeterError::BuildStreamError(format!(
                    "oväntat sample format: {other:?}"
                )));
            }
        };

        stream
            .play()
            .map_err(|e| VolumeMeterError::PlayStreamError(e.to_string()))?;

        Ok(Self { _stream: stream })
    }
}

fn maybe_emit(
    last: &AtomicU64,
    min_interval: Duration,
    cb: &Arc<dyn Fn(f32) + Send + Sync>,
    rms: f32,
) {
    let now_ns = now_monotonic_ns();
    let last_ns = last.load(Ordering::Relaxed);
    if now_ns.saturating_sub(last_ns) < min_interval.as_nanos() as u64 {
        return;
    }
    last.store(now_ns, Ordering::Relaxed);
    tracing::trace!("volume emit: rms={:.4}", rms);
    cb(rms);
}

fn now_monotonic_ns() -> u64 {
    // Använd Instant differencing mot fast nollpunkt per process.
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}

fn rms_f32(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

fn rms_i16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let norm = i16::MAX as f32;
    let sum_sq: f32 = samples
        .iter()
        .map(|&s| {
            let f = s as f32 / norm;
            f * f
        })
        .sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

fn rms_u16(samples: &[u16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    // u16 är offset-binär; centrera runt 32768.
    let center = 32768.0_f32;
    let norm = 32767.0_f32;
    let sum_sq: f32 = samples
        .iter()
        .map(|&s| {
            let f = (s as f32 - center) / norm;
            f * f
        })
        .sum();
    (sum_sq / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}
