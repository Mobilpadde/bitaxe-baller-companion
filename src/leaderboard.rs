use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use log::warn;

use crate::device::SystemInfo;

const SUBMIT_URL: &str = "https://bitaxeballer.com/api/leaderboard/submit";
// bitaxe-baller's reference client submits every 300s regardless of a new best,
// almost certainly to keep `last_seen` fresh for the site's "24h polling activity"
// prize-eligibility rule (https://bitaxeballer.com/leaderboard-rules.html). We submit
// immediately on a new career-best bestDiff (the site only *displays* on new bests
// anyway), and otherwise fall back to this much longer keep-alive so last_seen doesn't
// go stale - cuts routine request volume ~6x vs the reference client.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30 * 60);
const DISPLAY_NAME_MAX: usize = 30;
const MODEL_MAX: usize = 40;

/// Free-tier leaderboard submitter, mirroring bitaxe-baller's
/// `_leaderboard_submit_one` (see app.py): same payload shape, same
/// fire-and-forget error handling. Pro/license_key auth is out of scope here -
/// there's no UI on a headless device to buy or enter a license key, so only
/// the free (install_uuid + email) path applies.
pub struct Leaderboard {
    install_uuid: String,
    email: String,
    display_name: String,
    last_submit: HashMap<String, Instant>,
    // Highest career bestDiff seen per mac since this device booted - matches the
    // rules doc: "tracks your best share since the device last rebooted."
    best_diff_seen: HashMap<String, f64>,
}

impl Leaderboard {
    /// `None` if `LEADERBOARD_EMAIL` / `LEADERBOARD_DISPLAY_NAME` aren't both set -
    /// this feature is opt-in, same as in bitaxe-baller.
    pub fn from_env(install_uuid: String) -> Option<Self> {
        let email = option_env!("LEADERBOARD_EMAIL")
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let display_name: String = option_env!("LEADERBOARD_DISPLAY_NAME")
            .unwrap_or("")
            .trim()
            .chars()
            .take(DISPLAY_NAME_MAX)
            .collect();
        if email.is_empty() || display_name.is_empty() {
            return None;
        }
        Some(Self {
            install_uuid,
            email,
            display_name,
            last_submit: HashMap::new(),
            best_diff_seen: HashMap::new(),
        })
    }

    pub fn maybe_submit(&mut self, info: &SystemInfo) {
        let Some(mac) = info.mac_addr.clone().filter(|m| !m.is_empty()) else {
            return;
        };
        let cur_best = info.best_diff.as_deref().map(parse_diff).unwrap_or(0.0);
        let prev_best = self.best_diff_seen.get(&mac).copied();
        let is_new_best = prev_best.is_some_and(|prev| cur_best > prev);
        self.best_diff_seen
            .insert(mac.clone(), cur_best.max(prev_best.unwrap_or(0.0)));

        let now = Instant::now();
        if !is_new_best {
            if let Some(last) = self.last_submit.get(&mac) {
                if now.duration_since(*last) < KEEPALIVE_INTERVAL {
                    return;
                }
            }
        }
        match self.submit(&mac, info) {
            Ok(()) => {
                self.last_submit.insert(mac, now);
            }
            // Fire-and-forget: don't record a last-submit time on failure, so the
            // next 5s poll tick just retries naturally (matches the Python client).
            Err(e) => warn!("leaderboard submit for {mac} failed: {e:#}"),
        }
    }

    fn submit(&self, mac: &str, info: &SystemInfo) -> Result<()> {
        let model: String = info
            .asic_model
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(MODEL_MAX)
            .collect();

        let body = serde_json::json!({
            "display_name": self.display_name,
            "mac_addr": mac,
            "model": model,
            "best_diff_career": info.best_diff.as_deref().map(parse_diff).unwrap_or(0.0),
            "best_diff_session": info.best_session_diff.as_deref().map(parse_diff).unwrap_or(0.0),
            "hashrate_th_avg": info.hash_rate.unwrap_or(0.0) / 1000.0,
            "app_version": env!("CARGO_PKG_VERSION"),
            "install_uuid": self.install_uuid,
            "email": self.email,
        });
        let payload = serde_json::to_vec(&body)?;

        let config = HttpConfiguration {
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            timeout: Some(Duration::from_secs(8)),
            ..Default::default()
        };
        let mut client = HttpClient::wrap(EspHttpConnection::new(&config)?);
        let content_length = payload.len().to_string();
        let headers = [
            ("content-type", "application/json"),
            ("content-length", content_length.as_str()),
        ];
        let mut request = client.post(SUBMIT_URL, &headers)?;
        request.write_all(&payload)?;
        request.flush()?;
        let response = request.submit()?;
        let status = response.status();
        if !(200..300).contains(&status) {
            anyhow::bail!("HTTP {status}");
        }
        Ok(())
    }
}

/// Mirrors bitaxe-baller's `_parse_diff`: AxeOS reports diffs either as plain
/// numbers or as suffixed strings like "1.68G" / "682.42M".
fn parse_diff(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    let mult = match s.chars().last().unwrap().to_ascii_uppercase() {
        'K' => 1e3,
        'M' => 1e6,
        'G' => 1e9,
        'T' => 1e12,
        'P' => 1e15,
        _ => 1.0,
    };
    let digits = if mult != 1.0 { &s[..s.len() - 1] } else { s };
    digits.parse::<f64>().map(|v| v * mult).unwrap_or(0.0)
}
