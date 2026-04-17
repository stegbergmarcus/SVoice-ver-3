use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::resample::{mix_to_mono, resample_linear};
use crate::ringbuffer::AudioRing;

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

        let ring_cb = ring.clone();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &stream_cfg,
                    move |data: &[f32], _| {
                        let mono = mix_to_mono(data, channels);
                        let resampled = resample_linear(&mono, sample_rate, 16000);
                        ring_cb.push_samples(&resampled);
                    },
                    err_cb,
                    None,
                )
                .map_err(|e| CaptureError::Cpal(e.to_string()))?,
            cpal::SampleFormat::I16 => {
                let norm = i16::MAX as f32;
                device
                    .build_input_stream(
                        &stream_cfg,
                        move |data: &[i16], _| {
                            let f: Vec<f32> = data.iter().map(|&s| s as f32 / norm).collect();
                            let mono = mix_to_mono(&f, channels);
                            let resampled = resample_linear(&mono, sample_rate, 16000);
                            ring_cb.push_samples(&resampled);
                        },
                        err_cb,
                        None,
                    )
                    .map_err(|e| CaptureError::Cpal(e.to_string()))?
            }
            other => return Err(CaptureError::UnsupportedFormat(other)),
        };
        stream.play().map_err(|e| CaptureError::Cpal(e.to_string()))?;

        Ok(Self { _stream: stream, ring, sample_rate, channels })
    }
}
