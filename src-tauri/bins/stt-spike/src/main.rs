// STT-spike för iter 1 kör via Python-subprocess (faster-whisper) pga
// Windows Smart App Control blockerar osignerade native build-scripts
// (ct2rs, whisper-rs). Se bins/stt-spike/python/spike.py.
fn main() -> anyhow::Result<()> {
    println!("Kör spiken via: python bins/stt-spike/python/spike.py <wav-path>");
    println!("Se docs/superpowers/specs/2026-04-16-stt-spike-report.md när klar.");
    Ok(())
}
