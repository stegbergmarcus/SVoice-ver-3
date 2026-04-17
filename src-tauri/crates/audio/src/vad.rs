/// Hittar index för första och sista "icke-tysta" samplen baserat på RMS över 20ms-fönster.
/// Returnerar (start, end) i samples. Om allt är tyst returneras (0, 0).
pub fn trim_silence(samples: &[f32], sample_rate: u32, energy_threshold: f32) -> (usize, usize) {
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
    (first.unwrap_or(0), last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_silence_returns_zero_range() {
        let samples = vec![0.0; 16000];
        assert_eq!(trim_silence(&samples, 16000, 0.01), (0, 0));
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        let mut samples = vec![0.0; 16000];
        for i in 4000..8000 {
            samples[i] = 0.5;
        }
        let (start, end) = trim_silence(&samples, 16000, 0.01);
        assert!(start <= 4000 && start >= 3000);
        assert!(end >= 8000 && end <= 9000);
    }
}
