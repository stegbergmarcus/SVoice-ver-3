//! Enumerering av tillgängliga mic-enheter. Används av Settings-UI för
//! dropdown-val istället för free-text input.

use cpal::traits::{DeviceTrait, HostTrait};

/// Returnerar en lista med namn på alla input-enheter som cpal kan se.
/// Default-enheten listas alltid först med prefix "default: ".
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(|| "okänd default".into());

    let mut out: Vec<String> = Vec::new();
    out.push(format!("default: {default_name}"));

    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                // Undvik dubbel-listning av default.
                if name != default_name {
                    out.push(name);
                }
            }
        }
    }
    out
}
