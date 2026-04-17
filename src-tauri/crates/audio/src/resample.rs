pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz {
        return input.to_vec();
    }
    let ratio = from_hz as f32 / to_hz as f32;
    let out_len = ((input.len() as f32) / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_idx = i as f32 * ratio;
        let i0 = src_idx.floor() as usize;
        let i1 = (i0 + 1).min(input.len().saturating_sub(1));
        let frac = src_idx - i0 as f32;
        out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
    }
    out
}

/// Om input är stereo/multi-channel, mixa ner till mono genom genomsnitt.
pub fn mix_to_mono(input: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return input.to_vec();
    }
    let ch = channels as usize;
    input
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity_when_same_rate() {
        let s = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_linear(&s, 16000, 16000), s);
    }

    #[test]
    fn mix_stereo_to_mono_averages() {
        let stereo = vec![1.0, 3.0, 2.0, 4.0]; // L R L R
        assert_eq!(mix_to_mono(&stereo, 2), vec![2.0, 3.0]);
    }
}
