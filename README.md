# bitaxe-baller-companion

Standalone ESP32 firmware that polls a handful of [Bitaxe](https://github.com/bitaxeorg) miners
on the LAN and phones home the same two ways [bitaxe-baller](https://github.com/465media/bitaxe-baller) does from a PC:

- **Leaderboard** - HTTPS POST to `bitaxeballer.com/api/leaderboard/submit` on a new
  career-best share, plus a 30min keep-alive otherwise.
- **Relay** - a long-lived WSS connection to `relay.bitaxeballer.com` so the existing
  remote-dashboard/mobile clients can reach these Bitaxes without a PC running 24/7.

No local web UI, no tuning, no firmware flashing - read-only monitoring + phone-home only.
See the design writeup this was scaffolded from for the full protocol reference (envelope
format, auth, throttle semantics) - this repo re-implements those from scratch in Rust,
it does not share code with bitaxe-baller.

## Why `esp-idf-svc` (std), not `esp-hal`+Embassy (no_std)

Target hardware is a classic ESP32 (Xtensa dual-core, WROOM-32 module), which requires
Espressif's own rustc/LLVM fork (`espup`, toolchain channel `esp`) to compile Rust at all -
there's no stock-upstream-Rust option for Xtensa the way there is for the RISC-V chips
(C3/C6/H2).

- **`esp-idf-svc` (std, ESP-IDF-backed), not `esp-hal`+Embassy (no_std):** this project needs
  cert-validated HTTPS and a WSS client with custom headers (`Authorization: Bearer <token>`
  at handshake time). `esp-idf-svc::ws::client::EspWebSocketClient` wraps ESP-IDF's mature
  `esp_websocket_client` C component (mbedTLS-backed, real X.509 chain validation, supports
  custom handshake headers) and its HTTP client is equally mature. The no_std/Embassy path
  (`reqwless` + `embedded-tls`) explicitly does **not** support verified TLS in no_std as of
  this writing - closing that gap means reaching for the same mbedTLS C dependency anyway,
  just through a thinner-documented crate (`esp-mbedtls`). Revisit if that matures.

## Toolchain gotcha on this machine (and possibly yours)

If `esp`'s own `cargo` binary is ahead of the rustup shim on `PATH`:

```
$ which cargo
$HOME/.rustup/toolchains/esp/bin/cargo   # <- the Xtensa-fork cargo, NOT the rustup proxy
```

it happens to work here since this project *wants* the `esp` toolchain anyway, but it's
still not reading `rust-toolchain.toml` (toolchain-override resolution is a rustup *proxy*
feature; a toolchain's own `cargo` has no idea about it) - it's only right by coincidence.
Build through the actual rustup proxy so an override always resolves correctly:

```bash
~/.cargo/bin/cargo build --release
```

If you ever move or rename this directory, `cargo build` will fail with something like
`Failed to list cmake-file-api reply directory` - CMake bakes the absolute build path into
`target/xtensa-esp32-espidf`'s cache. Fix: `rm -rf target/xtensa-esp32-espidf` and rebuild
(the downloaded ESP-IDF SDK itself doesn't need to be re-fetched).

## One-time setup

```bash
cargo install espup ldproxy espflash   # espflash only needed once you're ready to flash hardware
espup install                          # installs the `esp` toolchain (Xtensa rustc/LLVM fork) via rustup
. $HOME/export-esp.sh                  # sets LIBCLANG_PATH etc. - needed in every new shell
```

First `~/.cargo/bin/cargo build` also triggers a **one-time download of the ESP-IDF SDK**
(pinned to v5.5.3 in `.cargo/config.toml`, ~1-2GB with its toolchain/submodules) into
`ESP_IDF_TOOLS_INSTALL_DIR` (`workspace/`, see `.cargo/config.toml`). Expect this to take a
while on first build.

```bash
export WIFI_SSID="your-ssid"
export WIFI_PASS="your-password"
export BITAXE_IPS="192.168.1.223,192.168.1.224"   # comma-separated, at least one

# Optional - leaderboard submission is silently disabled unless BOTH are set:
export LEADERBOARD_EMAIL="you@example.com"
export LEADERBOARD_DISPLAY_NAME="your-name"

~/.cargo/bin/cargo build --release
~/.cargo/bin/cargo run --release   # builds, flashes over USB, and opens the serial monitor
```

## Known gap: flash partition sizing

Rust std + WiFi + mbedTLS + HTTP + WS binaries commonly land in the 1.5-2.5MB range,
which can overflow ESP-IDF's default single-app partition table (~1MB factory partition
on some layouts). This board's factory partition is actually ~4MB (`0x3f0000`), and v0
(plain HTTP only) used 1,136,352 bytes (27.5%); adding the leaderboard's HTTPS client
(mbedTLS cert-bundle validation) only pushed that to ~1.3MB (~31%) since mbedTLS was
already linked in for WPA3-SAE - comfortably within budget. Revisit if the v2 relay WSS
client (another long-lived mbedTLS connection + its own buffers) pushes this meaningfully
higher; no custom partition table has been set up (see git history for a reverted attempt
at `CONFIG_PARTITION_TABLE_CUSTOM`, abandoned because it wasn't needed yet).

## Status: v1 (leaderboard)

- [x] WiFi STA connect (`src/wifi.rs`)
- [x] Poll loop over `GET /api/system/info` for each configured IP, every 5s (`src/device.rs`)
- [x] Parse the subset of fields needed for phone-home reporting, log to serial
- [x] Leaderboard HTTPS POST (`src/leaderboard.rs`) - same payload shape and fire-and-forget
      error handling as bitaxe-baller's `_leaderboard_submit_one`, but a different throttle:
      submits immediately on a new career-best `bestDiff`, otherwise falls back to a 30min
      keep-alive (vs. the reference client's unconditional 300s) to keep `last_seen` fresh for
      the site's 24h-activity prize rule while cutting routine request volume ~6x. Free tier
      only (install_uuid + email) - no license_key/Pro path, since there's no UI on a headless
      device to buy or enter one.
- [x] install_uuid generated once and NVS-persisted (`src/config.rs`)
- [ ] Relay WSS client + in-RAM request router (**v2**)
- [ ] Captive-portal provisioning (WiFi creds + Bitaxe IPs are still compile-time env vars -
      fine for bring-up on your own hardware, not for anything you'd hand to someone else)

**Known simplification:** `hashrate_th_avg` in the leaderboard payload is the Bitaxe's
instantaneous hash rate, not bitaxe-baller's 15-minute rolling average - revisit with a
ring buffer per device if the noise turns out to matter for leaderboard ranking.

**Permanently out of scope:** tuning/pool-config writes, firmware flashing, history/charts,
multi-tenant fleets - see the design writeup for why.

## Verifying this actually works

v0 (WiFi + poll loop) is built and flashed to real ESP32 (WROOM-32) hardware: WiFi
connects, polls the Bitaxe every 5s, and `SystemInfo` parses cleanly (hash rate, temps,
power, shares, best diff) against a live BM1370 device.

v1's leaderboard client, including the reworked new-best/30min-keep-alive throttle (see
Status below), has been flashed and confirmed end-to-end against the real
`bitaxeballer.com` server across two separate flashes:

- Each flash's first poll tick triggers an immediate submit (empty throttle state) -
  confirmed both times via a single `esp-x509-crt-bundle: Certificate validated` line
  right after the first `SystemInfo` log, with no repeat submissions afterward while
  `bestDiff` stayed flat (no 5s/300s spam like the old always-throttle version had).
- `GET https://bitaxeballer.com/api/leaderboard/data?category=lucky` confirms server-side
  receipt both times: `hashrate_th_avg` exactly matches that boot's first-tick hash rate
  (e.g. `1297.0801/1000 = 1.2970801`), and `last_seen` advances from one flash to the next
  (`1783718976` -> `1783719937`) - proof the keep-alive mechanism is doing its job of
  refreshing server-side activity without spamming on every tick.

Not yet observed on hardware: a genuine `bestDiff` increase triggering an out-of-cycle
submit, and the 30min keep-alive actually firing (both require longer/luckier runs than
tested so far) - low-risk given the throttle logic itself is straightforward and unit-shaped,
but worth a longer-running check before calling v1 fully done.
