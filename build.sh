#!/usr/bin/env bash
# Build and flash to real ESP32 hardware over USB, then open the serial monitor.
set -euo pipefail

: "${WIFI_SSID:?set WIFI_SSID}"
: "${WIFI_PASS:?set WIFI_PASS}"
: "${BITAXE_IPS:?set BITAXE_IPS}"

# ponytail: espup's env file sets LIBCLANG_PATH etc.; harmless to re-source if already sourced.
[ -f "$HOME/export-esp.sh" ] && source "$HOME/export-esp.sh"

~/.cargo/bin/cargo run --release
