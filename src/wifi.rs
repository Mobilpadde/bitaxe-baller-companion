use anyhow::Result;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use log::info;

pub fn connect(wifi: &mut BlockingWifi<EspWifi<'static>>, ssid: &str, password: &str) -> Result<()> {
    let config = Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().expect("WIFI_SSID must be <= 32 bytes"),
        bssid: None,
        // This is a minimum threshold (maps to wifi_sta_config_t.threshold.authmode), not an
        // exact match - a WPA3-SAE AP is accepted too since it's stronger than WPA2Personal.
        auth_method: AuthMethod::WPA2Personal,
        password: password.try_into().expect("WIFI_PASS must be <= 64 bytes"),
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&config)?;
    wifi.start()?;
    info!("wifi started, connecting to {ssid}...");
    wifi.connect()?;
    wifi.wait_netif_up()?;
    info!("wifi up: {:?}", wifi.wifi().sta_netif().get_ip_info()?);
    Ok(())
}
