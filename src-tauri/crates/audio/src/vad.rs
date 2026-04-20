/// Hittar index för första och sista "icke-tysta" samplen baserat på RMS över
/// 20 ms-fönster. `pad_ms` expanderar start/slut-gränserna symmetriskt så att
/// tonlösa konsonanter (s, f, t, k) och ordslut inte kapas innan STT-modellen
/// får ljudet. Returnerar (start, end) i samples. Om allt är tyst returneras
/// (0, 0).
pub fn trim_silence(
    samples: &[f32],
    sample_rate: u32,
    energy_threshold: f32,
    pad_ms: u32,
) -> (usize, usize) {
    let window = (sample_rate as usize / 50).max(1); // 20ms
    let mut first = None;
    let mut last = 0;
    for (i, chunk) in samples.chunks(window).enumerate() {
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        if rms > energy_threshold {
            if first.is_none() {
                first = Some(i * window);
            }
            last = i * window + chunk.len();
        }
    }
    let Some(raw_start) = first else {
        return (0, 0);
    };
    let pad_samples = (sample_rate as u64 * pad_ms as u64 / 1000) as usize;
    let start = raw_start.saturating_sub(pad_samples);
    let end = (last + pad_samples).min(samples.len());
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_silence_returns_zero_range() {
        let samples = vec![0.0; 16000];
        assert_eq!(trim_silence(&samples, 16000, 0.01, 0), (0, 0));
    }

    #[test]
    fn trims_leading_and_trailing_silence_without_padding() {
        let mut samples = vec![0.0; 16000];
        for i in 4000..8000 {
            samples[i] = 0.5;
        }
        let (start, end) = trim_silence(&samples, 16000, 0.01, 0);
        assert!(start <= 4000 && start >= 3000);
        assert!(end >= 8000 && end <= 9000);
    }

    #[test]
    fn padding_expands_boundaries_symmetrically() {
        let mut samples = vec![0.0; 16000];
        for i in 8000..9000 {
            samples[i] = 0.5;
        }
        // 250 ms pad @ 16 kHz = 4000 samples
        let (start, end) = trim_silence(&samples, 16000, 0.01, 250);
        let (raw_start, raw_end) = trim_silence(&samples, 16000, 0.01, 0);
        assert_eq!(start, raw_start.saturating_sub(4000));
        assert_eq!(end, (raw_end + 4000).min(16000));
    }

    #[test]
    fn padding_clamps_at_buffer_edges() {
        // Tal börjar vid sample 100 och slutar precis vid slutet → padding
        // ska clamp:as till [0, len].
        let mut samples = vec![0.0; 8000];
        for i in 100..8000 {
            samples[i] = 0.5;
        }
        let (start, end) = trim_silence(&samples, 16000, 0.01, 500);
        assert_eq!(start, 0);
        assert_eq!(end, 8000);
    }
}
