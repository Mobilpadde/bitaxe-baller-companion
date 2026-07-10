use anyhow::Result;
use embedded_svc::http::{client::Client as HttpClient, Method};
use embedded_svc::utils::io;
use esp_idf_svc::http::client::EspHttpConnection;
use serde::Deserialize;

/// Subset of AxeOS's `GET /api/system/info` we actually need for phone-home
/// reporting (leaderboard + relay). See bitaxe-baller's CLAUDE.md for the
/// full field list this is a deliberate subset of.
#[derive(Debug, Deserialize)]
pub struct SystemInfo {
    #[serde(rename = "hashRate")]
    pub hash_rate: Option<f64>,
    pub temp: Option<f64>,
    #[serde(rename = "vrTemp")]
    pub vr_temp: Option<f64>,
    pub power: Option<f64>,
    #[serde(rename = "bestDiff", deserialize_with = "de_opt_diff")]
    pub best_diff: Option<String>,
    #[serde(rename = "bestSessionDiff", deserialize_with = "de_opt_diff")]
    pub best_session_diff: Option<String>,
    #[serde(rename = "sharesAccepted")]
    pub shares_accepted: Option<u64>,
    #[serde(rename = "sharesRejected")]
    pub shares_rejected: Option<u64>,
    #[serde(rename = "macAddr")]
    pub mac_addr: Option<String>,
    #[serde(rename = "ASICModel")]
    pub asic_model: Option<String>,
    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: Option<u64>,
}

// ponytail: some AxeOS firmware versions send bestDiff/bestSessionDiff as a raw
// number instead of a formatted string ("911.5M") - accept either.
fn de_opt_diff<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum DiffValue {
        Str(String),
        Num(f64),
    }
    Ok(Option::<DiffValue>::deserialize(deserializer)?.map(|v| match v {
        DiffValue::Str(s) => s,
        DiffValue::Num(n) => n.to_string(),
    }))
}

/// Polls one Bitaxe over plain LAN HTTP (no TLS - this never leaves the LAN).
/// Buffer is sized generously for AxeOS's ~1.5-2.5KB response body; a reply
/// larger than the buffer just has its trailing fields dropped rather than
/// erroring, which is fine since every field here is `Option`.
pub fn poll(ip: &str) -> Result<SystemInfo> {
    let url = format!("http://{ip}/api/system/info");
    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default())?);
    let request = client.request(Method::Get, &url, &[])?;
    let mut response = request.submit()?;

    let status = response.status();
    if status != 200 {
        anyhow::bail!("HTTP {status}");
    }

    let mut buf = vec![0u8; 4096];
    let bytes_read = io::try_read_full(&mut response, &mut buf).map_err(|e| e.0)?;
    let info: SystemInfo = serde_json::from_slice(&buf[..bytes_read])?;
    Ok(info)
}
