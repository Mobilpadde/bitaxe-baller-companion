use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use log::{info, warn};

mod device;
mod wifi;

// v0 bring-up: WiFi + Bitaxe IPs are compile-time env vars (see README).
// v1 replaces this with a captive-portal config flow + NVS persistence.
const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASS: &str = env!("WIFI_PASS");
const BITAXE_IPS: &str = env!("BITAXE_IPS");

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut esp_wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;
    wifi::connect(&mut esp_wifi, WIFI_SSID, WIFI_PASS)?;

    let ips: Vec<&str> = BITAXE_IPS
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if ips.is_empty() {
        anyhow::bail!("BITAXE_IPS is empty - set it to at least one Bitaxe IP (see README)");
    }
    info!("polling {} device(s): {ips:?}", ips.len());

    loop {
        for ip in &ips {
            match device::poll(ip) {
                Ok(snapshot) => info!("{ip}: {snapshot:?}"),
                Err(e) => warn!("{ip}: poll failed: {e:#}"),
            }
        }
        std::thread::sleep(core::time::Duration::from_secs(5));
    }
}
