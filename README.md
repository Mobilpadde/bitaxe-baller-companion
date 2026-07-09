# bitaxe-companion-fw

Standalone ESP32-C3 firmware that polls a handful of [Bitaxe](https://github.com/bitaxeorg) miners
on the LAN and phones home the same two ways [bitaxe-baller](https://github.com/) does from a PC:

- **Leaderboard** - periodic HTTPS POST to `bitaxeballer.com/api/leaderboard/submit`.
- **Relay** - a long-lived WSS connection to `relay.bitaxeballer.com` so the existing
  remote-dashboard/mobile clients can reach these Bitaxes without a PC running 24/7.

No local web UI, no tuning, no firmware flashing - read-only monitoring + phone-home only.
See the design writeup this was scaffolded from for the full protocol reference (envelope
format, auth, throttle semantics) - this repo re-implements those from scratch in Rust/C,
it does not share code with bitaxe-baller.

## Why ESP32-C3 + `esp-idf-svc` (std), not Xtensa / no_std

- **RISC-V (C3/C6), not Xtensa (classic ESP32/S2/S3):** Xtensa chips need Espressif's own
  rustc/LLVM fork (`espup`) to compile Rust at all. RISC-V chips build with **stock,
  upstream Rust** (still nightly, for `-Z build-std` against the custom
  `riscv32imc-esp-espidf` target - see the gotcha below - but no vendor-forked compiler to
  track). Meaningfully less toolchain maintenance long-term.
- **`esp-idf-svc` (std, ESP-IDF-backed), not `esp-hal`+Embassy (no_std):** this project needs
  cert-validated HTTPS and a WSS client with custom headers (`Authorization: Bearer <token>`
  at handshake time). `esp-idf-svc::ws::client::EspWebSocketClient` wraps ESP-IDF's mature
  `esp_websocket_client` C component (mbedTLS-backed, real X.509 chain validation, supports
  custom handshake headers) and its HTTP client is equally mature. The no_std/Embassy path
  (`reqwless` + `embedded-tls`) explicitly does **not** support verified TLS in no_std as of
  this writing - closing that gap means reaching for the same mbedTLS C dependency anyway,
  just through a thinner-documented crate (`esp-mbedtls`). Revisit if that matures.

## Toolchain gotcha on this machine (and possibly yours)

`rustup toolchain list` shows an `esp` toolchain (Xtensa fork) installed *ahead of* the
rustup shim on `PATH`:

```
$ which cargo
/home/mc/.rustup/toolchains/esp/bin/cargo   # <- the Xtensa-fork cargo, NOT the rustup proxy
```

That binary does not read `rust-toolchain.toml` (toolchain-override resolution is a rustup
*proxy* feature; a toolchain's own `cargo` has no idea about it). Always build this project
through the actual rustup proxy, not whatever `cargo` resolves to first on `PATH`:

```bash
~/.cargo/bin/cargo build --release
```

(or fix your shell's `PATH` ordering so `~/.cargo/bin` comes first - out of scope for this
repo to change for you).

## One-time setup

```bash
~/.cargo/bin/rustup component add rust-src --toolchain nightly
cargo install ldproxy espflash   # espflash only needed once you're ready to flash hardware
```

First `~/.cargo/bin/cargo build` also triggers a **one-time download of the ESP-IDF SDK**
(pinned to v5.5.3 in `.cargo/config.toml`, ~1-2GB with its toolchain/submodules) into
`ESP_IDF_TOOLS_INSTALL_DIR` (`workspace/`, see `.cargo/config.toml`). Expect this to take a
while on first build.

```bash
export WIFI_SSID="your-ssid"
export WIFI_PASS="your-password"
export BITAXE_IPS="192.168.1.223,192.168.1.224"   # comma-separated, at least one

~/.cargo/bin/cargo build --release
~/.cargo/bin/cargo run --release   # builds, flashes over USB, and opens the serial monitor
```

## Status: v0 (bring-up)

- [x] WiFi STA connect (`src/wifi.rs`)
- [x] Poll loop over `GET /api/system/info` for each configured IP, every 5s (`src/device.rs`)
- [x] Parse the subset of fields needed for phone-home reporting, log to serial
- [ ] Leaderboard HTTPS POST (**v1**)
- [ ] Relay WSS client + in-RAM request router (**v2**)
- [ ] Captive-portal provisioning, NVS-persisted config, install_uuid (**v1**, currently
      WiFi creds + IPs are compile-time env vars, baked into the binary - fine for bring-up
      on your own hardware, not for anything you'd hand to someone else)

**Permanently out of scope:** tuning/pool-config writes, firmware flashing, history/charts,
multi-tenant fleets - see the design writeup for why.

## Verifying this actually works

This has been scaffolded and reviewed against the current `esp-idf-svc` examples
(`wifi.rs`, `http_client.rs`) but **not yet built or flashed** - no ESP-IDF SDK was
downloaded and no hardware was on hand while writing it. Before trusting it:

1. `~/.cargo/bin/cargo build --release` and fix whatever the compiler flags (dependency
   version drift is the likeliest culprit - `esp-idf-svc` moves fast).
2. Flash to real ESP32-C3 hardware, confirm the serial monitor shows successful WiFi
   connect + per-device JSON parsed correctly against your actual Bitaxe(s).
