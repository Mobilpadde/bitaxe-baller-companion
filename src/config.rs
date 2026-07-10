use anyhow::Result;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::sys::esp_random;

const NAMESPACE: &str = "cfg";
const KEY_INSTALL_UUID: &str = "install_uuid";

/// Stable per-install identifier for free-tier leaderboard auth - generated once on
/// first boot, persisted in NVS. Not a hardware fingerprint: erasing NVS (or a fresh
/// flash) gets a new uuid, matching bitaxe-baller's own install_uuid semantics.
pub fn install_uuid(nvs: EspDefaultNvsPartition) -> Result<String> {
    let nvs = EspNvs::new(nvs, NAMESPACE, true)?;
    let mut buf = [0u8; 40];
    if let Some(existing) = nvs.get_str(KEY_INSTALL_UUID, &mut buf)? {
        if !existing.is_empty() {
            return Ok(existing.to_string());
        }
    }
    let fresh = random_uuid_v4();
    nvs.set_str(KEY_INSTALL_UUID, &fresh)?;
    Ok(fresh)
}

// ponytail: hand-rolled instead of pulling in the `uuid`+`getrandom` crates - just
// ESP-IDF's HRNG (`esp_random`) plus the standard UUIDv4 version/variant bits.
fn random_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(4) {
        chunk.copy_from_slice(&unsafe { esp_random() }.to_ne_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}
