# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Standalone Rust firmware for a real ESP32 (Xtensa, WROOM-32 module) that polls a handful of
[Bitaxe](https://github.com/bitaxeorg) miners on the LAN and phones home the same two ways
[bitaxe-baller](https://github.com/465media/bitaxe-baller) does from a PC: a leaderboard HTTPS
POST, and (not yet implemented) a relay WSS connection so remote dashboard/mobile clients can
reach these Bitaxes without a PC running 24/7. No local web UI, no tuning, no firmware flashing -
read-only monitoring + phone-home only. This repo re-implements bitaxe-baller's leaderboard/relay
protocol from scratch in Rust; it does not share code with bitaxe-baller.

## Build & flash

```bash
export WIFI_SSID="your-ssid"
export WIFI_PASS="your-password"
export BITAXE_IPS="192.168.1.223,192.168.1.224"   # comma-separated, at least one

# Optional - leaderboard submission is silently disabled unless BOTH are set:
export LEADERBOARD_EMAIL="you@example.com"
export LEADERBOARD_DISPLAY_NAME="your-name"

~/.cargo/bin/cargo build --release
~/.cargo/bin/cargo run --release   # builds, flashes over USB, opens the serial monitor
# or: ./build.sh (wraps the same env-var checks + cargo run)
```

**Always invoke `~/.cargo/bin/cargo`, not whatever `cargo` resolves to on `PATH`.** The `esp`
toolchain's own `cargo` binary is commonly ahead of the rustup shim on `PATH`, and it does not
read `rust-toolchain.toml` overrides (toolchain-override resolution is a rustup *proxy* feature).

`WIFI_SSID`/`WIFI_PASS`/`BITAXE_IPS` are compile-time env vars baked into the binary via `env!()`
in `src/main.rs` - there is no captive-portal provisioning yet (planned, not started). Build fails
outright if any are unset.

### One-time environment setup

```bash
cargo install espup ldproxy espflash
espup install                          # installs the `esp` toolchain (Xtensa rustc/LLVM fork)
. $HOME/export-esp.sh                  # sets LIBCLANG_PATH etc. - needed in every new shell
```

First build also triggers a one-time download of the ESP-IDF SDK (pinned to v5.5.3 in
`.cargo/config.toml`, ~1-2GB). `.cargo/config.toml` sets `ESP_IDF_TOOLS_INSTALL_DIR = "workspace"`,
which despite the name lands in `.embuild/`, not a literal `workspace/` folder - that's an
`esp-idf-sys` build-script keyword, not a path (confirmed against its `common.rs` source).

### Known gotchas

- **Moving/renaming this directory breaks the build**: `cargo build` fails with `Failed to list
  cmake-file-api reply directory` because CMake bakes the absolute build path into
  `target/xtensa-esp32-espidf`'s cache. Fix: `rm -rf target/xtensa-esp32-espidf` and rebuild (the
  downloaded ESP-IDF SDK itself doesn't need to be re-fetched).
- Target is Xtensa (`xtensa-esp32-espidf`), which requires Espressif's own rustc/LLVM fork
  (`rust-toolchain.toml` pins `channel = "esp"`) - there is no stock-upstream-Rust option for
  Xtensa the way there is for RISC-V chips (C3/C6/H2). Don't "fix" the toolchain channel back to
  `nightly`; that only works for RISC-V targets.

## Architecture

Single binary, no async runtime - a synchronous loop on the main thread, one module per concern:

- **`src/wifi.rs`** - blocking WiFi STA connect (`BlockingWifi<EspWifi>`).
- **`src/device.rs`** - polls one Bitaxe's `GET /api/system/info` (plain HTTP, LAN-only, no TLS)
  and deserializes the subset of AxeOS's response needed for phone-home reporting into
  `SystemInfo`. Some AxeOS firmware versions send `bestDiff`/`bestSessionDiff` as a raw number
  instead of a formatted string ("911.5M") - `de_opt_diff` accepts either shape.
- **`src/config.rs`** - generates and NVS-persists a stable `install_uuid` (hand-rolled UUIDv4 from
  `esp_random()`, not the `uuid`/`getrandom` crates, to save flash). Not a hardware fingerprint:
  erasing NVS or a fresh flash gets a new uuid, matching bitaxe-baller's own semantics.
  `src/main.rs` clones the `EspDefaultNvsPartition` before handing one copy to `EspWifi::new` and
  the other to `config::install_uuid` - both need their own handle.
- **`src/leaderboard.rs`** - HTTPS POST to `bitaxeballer.com/api/leaderboard/submit`, mirroring
  bitaxe-baller's `_leaderboard_submit_one` payload shape (see `app.py` in the bitaxe-baller repo
  for the reference implementation) but with a different throttle: submits immediately on a new
  career-best `bestDiff`, otherwise falls back to a 30min keep-alive instead of the reference
  client's unconditional 300s - keeps `last_seen` fresh for the site's 24h-activity prize rule
  (see `leaderboard-rules.html`) while cutting routine request volume ~6x. Free tier only
  (`install_uuid` + email) - no `license_key`/Pro path, since there's no UI on a headless device to
  buy or enter one. `hashrate_th_avg` is the Bitaxe's instantaneous hash rate, not bitaxe-baller's
  15-minute rolling average (known simplification - revisit with a ring buffer per device if noise
  turns out to matter for ranking).
- **`src/main.rs`** - wires it together: connect WiFi, resolve `install_uuid`, then loop forever:
  poll every configured Bitaxe IP every 5s, log the result, feed successful polls to
  `Leaderboard::maybe_submit`.

Uses `esp-idf-svc` (std, ESP-IDF-backed), not `esp-hal`+Embassy (no_std): this project needs
cert-validated HTTPS and (eventually) a WSS client with custom handshake headers
(`Authorization: Bearer <token>`). `esp-idf-svc`'s HTTP client and `EspWebSocketClient` wrap
ESP-IDF's mature, mbedTLS-backed C components with real X.509 validation. The no_std/Embassy path
(`reqwless` + `embedded-tls`) does not support verified TLS in no_std as of this writing.

HTTPS cert validation uses ESP-IDF's built-in trusted-root bundle
(`crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach)`), not a pinned/custom cert.

## Status (see README.md "Status" section for the authoritative up-to-date checklist)

v1 (leaderboard) is implemented and confirmed working against real hardware and the live
`bitaxeballer.com` server. Not yet implemented: the relay WSS client (v2) and captive-portal
provisioning - WiFi creds and Bitaxe IPs remain compile-time env vars, fine for bring-up on your
own hardware but not for handing this to someone else.

**Permanently out of scope:** tuning/pool-config writes, firmware flashing, history/charts,
multi-tenant fleets.

## Flash budget

Rust std + WiFi + mbedTLS + HTTP binaries commonly land in the 1.5-2.5MB range, which can overflow
ESP-IDF's default partition table on some layouts. This board's factory partition is ~4MB
(`0x3f0000`); current usage is ~31% (~1.3MB) with the leaderboard's HTTPS client included. No
custom partition table is set up (a `CONFIG_PARTITION_TABLE_CUSTOM` attempt was tried and reverted
- see git history - because it wasn't needed). Re-check this budget before adding the v2 relay WSS
client (another long-lived mbedTLS connection + its own buffers).
