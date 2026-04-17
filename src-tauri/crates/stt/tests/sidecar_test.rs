use std::path::PathBuf;

use svoice_stt::Sidecar;

#[tokio::test]
#[ignore] // kräver systempython — kör manuellt med `cargo test -p svoice-stt -- --ignored`
async fn sidecar_responds_ready_on_spawn() {
    let python = PathBuf::from("py");
    let script = PathBuf::from("../../resources/python/stt_sidecar.py");
    let sidecar = Sidecar::spawn(&python, &[], &script).await.expect("spawn");
    // Om vi nådde hit utan panic har vi fått Ready.
    sidecar.shutdown().await.expect("shutdown");
}
