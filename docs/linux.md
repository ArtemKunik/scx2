# Linux support — delivery specification

**Status:** proposed. This is the hand-off spec for the first Linux release of
`scx2`. It covers what is delivered, how it is built, what must be verified on
hardware, and what evidence comes back.

**Primary target:** Ubuntu 24.04 LTS and Debian 12, x86_64, Wayland and X11.

**Secondary target:** Fedora 40+ and Arch, best effort. Same code, different
package names.

**Not in scope:** protocol changes, new amp features, packaging (`.deb`,
AppImage, Flatpak), macOS. The protocol layer is already OS-agnostic, so this
work is platform plumbing, not a port.

## 1. Deliverables

| ID | Deliverable | Done when |
| --- | --- | --- |
| D1 | Clean build on Linux | `cargo build --release` from a fresh Ubuntu 24.04 with the documented package list |
| D2 | CLI parity | Every command in the README works; offline commands match the Windows output byte-for-byte |
| D3 | Editor | Opens, draws and edits under Wayland and X11; file dialogs work through the desktop portal |
| D4 | Audio | Tuner and recorder list the amp's USB capture input and record a valid WAV |
| D5 | Device access | The amp is reachable as a normal user after installing the udev rule, no root |
| D6 | CI | `ubuntu-latest` job runs the test suite and a release build |
| D7 | Documentation | README build section, `docs/test-plan.md`, this file |
| D8 | Evidence | The artifacts in §7 returned from a real amp |
| D9 | Linux captures | `scx2 replay` reads Linux `usbmon` (link type 220) as well as USBPcap |

D1–D3 and D6 are shippable without an amp. D4, D5 and D8 need one Linux
machine with the amp attached.

## 2. Code changes

| File | Change |
| --- | --- |
| `Cargo.toml` | Move `windows` to `[target.'cfg(windows)'.dependencies]`; use `hidapi`'s `basic-udev` backend on Linux; keep `tuner = ["dep:cpal", "dep:windows", "dep:hound"]` |
| `src/endpoint.rs` | The module is Windows-only in meaning. Keep the stubs or gate the module; no "Enable amp input" off Windows |
| `src/ui/tuner.rs` | Hide the **Enable amp input** button (`endpoint::can_enable()`) and drop the Windows hover text |
| `src/transport.rs` | Platform-neutral error text; prefer the vendor HID interface in `scan`/`open` (composite device) |
| `src/windows.rs` | Keep the stubs; `report_ui_error` should fall back to `eprintln!` off Windows |
| `src/recorder.rs` | No change expected — `HOME` and `xdg-open` branches already exist. Verify the take path and folder opening |
| `src/capture.rs` | Add the Linux `usbmon` (link type 220) reader; keep USBPcap |
| `packaging/udev/70-scx2.rules` | New |
| `.github/workflows/linux.yml`, `azure-pipelines.yml` | New; Azure is the current origin, GitHub is kept for a mirror |
| `docs/test-plan.md`, `README.md` | Linux prerequisites and paths |

Do not change the protocol, frame builders, `.fuse` handling, or the UI layout.

## 3. Build prerequisites

Ubuntu 24.04 / Debian 12:

```console
sudo apt install build-essential pkg-config libasound2-dev \
  libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo build --release
```

Fedora 40+:

```console
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config \
  alsa-lib-devel libxkbcommon-devel wayland-devel libX11-devel \
  mesa-libGL-devel libxcb-devel
```

Arch:

```console
sudo pacman -S --needed base-devel pkgconf alsa-lib \
  libxkbcommon wayland libx11 mesa libxcb
```

Why these:

* `hidapi` needs no extra package: the Linux build selects the pure-Rust
  `basic-udev` backend, which reads `/sys/class/hidraw` directly. Switching back
  to the C hidraw backend (`features = ["linux-static-hidraw"]`) would require
  `libudev-dev`/`systemd-devel`. It does not use libusb and does not need a
  driver swap.
* `alsa-lib` — `cpal` uses ALSA on Linux. PipeWire and PulseAudio both provide
  ALSA compatibility, so no extra runtime setup is usually needed.
* X11/Wayland/GL — `eframe`/`winit`/`glutin`.

Runtime-only requirements: a working desktop session (X11 or Wayland),
PipeWire or PulseAudio, and `xdg-desktop-portal` with a backend (needed by
`rfd` for the file dialogs).

## 4. Device access (udev)

`/dev/hidraw*` is root-only by default. Install:

```udev
# /etc/udev/rules.d/70-scx2.rules
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1ed8", ATTRS{idProduct}=="000e", TAG+="uaccess"
```

```console
sudo udevadm control --reload-rules
sudo udevadm trigger --action=add --subsystem-match=hidraw
# a plain `udevadm trigger` sends change events, which keep the old
# permissions; `--action=add` re-runs the rule and applies the ACL.
# Replugging the amp works too. Then:
ls -l /dev/hidraw*        # shows + for the logged-in user via uaccess
```

`TAG+="uaccess"` relies on systemd-logind. On systems without it, use
`MODE="0660", GROUP="plugdev"` (Debian/Ubuntu) or the local `wheel` group, and
add the user to that group.

The boot-mode product id `1ed8:0006` gets the same treatment when firmware
update work (M5.2) starts.

## 5. Behaviour differences to document

* **No "disabled endpoint" concept.** ALSA publishes the amp's capture device
  when it is plugged; there is nothing to enable. The Windows-only button is
  hidden, and the tuner falls back to "Auto" / the device picker.
* **Audio names.** `is_amp_input` matches `"superchamp"`, `"super champ"` and
  `"internal aux jack"` and `"card=amp"`. ALSA names the card from the USB
  product string, so the amp's PCMs read `hw:CARD=Amp,DEV=0`; the last hint
  matches that form.
* **File dialogs.** `rfd` defaults to the XDG portal on Linux, so no GTK build
  dependency. A session without a portal falls back to typing a path (existing
  UI behaviour).
* **Captures.** `replay` reads USBPcap (link type 249) and Linux `usbmon`
  mmapped files (link type 220). `usbmon` needs `modprobe usbmon` and
  `tshark`/`dumpcap`; the decoder covers the interrupt reports the protocol
  uses, not every control or isochronous transfer.
* **Console.** `detach_console_if_owner` is a no-op; launched from a terminal,
  errors stay on stderr.

## 6. Acceptance criteria

### Offline, no amp (gate for D1–D3, D6)

1. `cargo build --release` succeeds on a clean Ubuntu 24.04 with §3's packages.
2. `cargo test` passes, including the checked-in capture tests.
3. `scx2 replay captures/fuse-scx2-2021-06-07.pcap -o reference.json` produces
   the same file as on Windows (`sha256sum` matches).
4. `scx2 show`, `diff`, `import`, `export` produce the same output/files as on
   Windows for the same inputs.
5. `scx2` with no arguments opens the editor; `Open`/`Save As` work through the
   portal; the window is usable under both `WINIT_UNIX_BACKEND=wayland` and
   `WINIT_UNIX_BACKEND=x11`.
6. `scx2 scan` with no amp exits cleanly and lists no Fender device.

### With the amp (gate for D4, D5, D8)

7. `scx2 scan --all` lists `1ed8:000e` as a normal user, without root.
8. `scx2 dump -o linux-backup.json` completes and the preset table matches the
   Windows dump for the same amp.
9. `scx2 monitor` shows the live reports; `scx2 learn` attributes one offset
   per knob turn.
10. A dry-run write (`scx2 apply amp --offset 44 --value 0x00`) prints the
    frames; with `--yes` it changes the live buffer and a fresh dump shows it.
11. `scx2 store` and `scx2 rename` against an empty slot work and survive a
    power cycle.
12. The tuner sees the amp input and reports notes; a 5-second recorder take is
    a valid 24-bit mono WAV with a sidecar.
13. No unexpected disconnect or kernel log (`dmesg`) error during the session.

## 7. Evidence for delivery

Return one archive named `scx2-linux-<distro>-<date>.tar.gz` containing:

| File | How |
| --- | --- |
| `system.txt` | `uname -a`, `lsb_release -a`, `cargo --version`, `rustc --version`, `echo $XDG_SESSION_TYPE` |
| `usb.txt` | `lsusb -d 1ed8: -v` and `ls -l /dev/hidraw*` |
| `scan.txt` | `scx2 scan --all` |
| `dump.json` | `scx2 dump -o dump.json` and `sha256sum dump.json` |
| `monitor.txt` | 10 seconds of `scx2 monitor` while turning the volume knob |
| `learn.txt` | `scx2 learn` lines, one knob at a time |
| `audio.txt` | tuner/recorder device list and the amp input name |
| `editor-wayland.png`, `editor-x11.png` | one screenshot each |
| `take.wav`, `take.json` | a 5-second recorder take |
| `usbmon.pcap` (optional) | see below |

Capture procedure (optional, for protocol debugging):

```console
sudo modprobe usbmon
sudo tshark -D                       # note the amp's bus, e.g. usbmon1
lsusb -d 1ed8:000e                   # "Bus 001 Device 005: ..."
sudo tshark -i usbmon1 -w scx2-linux-$(date +%F).pcap
# start scx2 in another terminal, turn a knob, stop with Ctrl-C
```

Keep the capture short (under a minute) so the file stays small. `usbmon0`
captures every bus. The file replays with `scx2 replay` on any platform; the
same command also reads the bundled USBPcap reference.

If anything fails, include the command, the full stderr, and
`RUST_BACKTRACE=1` output. Do not include a modified amp state; restore from
`dump.json` first.

## 8. Sequencing

1. **Build and offline parity** — dependencies, `Cargo.toml` target deps, CI,
   README. No behaviour change on Windows. Gate: §6.1–6.6.
2. **Platform gates** — endpoint button, `report_ui_error`, neutral messages.
   Gate: editor smoke test on both display servers.
3. **Transport hardening** — interface selection in `scan`/`open`, plus a
   `--path` escape hatch for debugging. Gate: scan/dump on hardware.
4. **Device access** — udev rule and install note. Gate: §6.7–6.11.
5. **Audio** — name matching, tuner and recorder on hardware. Gate: §6.12–6.13.
6. **Evidence and docs** — this file, test plan, returned archive.

Effort: steps 1–2 are about a day, step 3 half a day, steps 4–5 one hardware
session, step 6 a few hours. The `usbmon` reader is included in step 1.

## 9. Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Amp's HID interface is not the first node on Linux | scan/dump fails | Interface-aware selection in step 3; `usbhid` binds the vendor collection generically, so this is small |
| hidraw permissions | "could not open the amp" | udev rule; check `ls -l /dev/hidraw*` |
| Audio card name differs from the Windows endpoint name | "Auto (amp USB)" misses it | Fall back to the device picker; extend `AMP_INPUT_HINTS` after the returned `audio.txt` |
| No portal in the session | file dialogs do nothing | Existing path entry; document `xdg-desktop-portal` |
| Wayland quirks in eframe 0.27 | window/input issues | `WINIT_UNIX_BACKEND=x11`; consider an eframe bump separately |
| No Linux hardware in CI | unverified HID path | CI covers offline; hardware gate is explicit in §6 |

## 10. Open questions

1. Which distro is the reference for the first release? This spec assumes
   Ubuntu 24.04.
2. Should the udev rule be shipped in the repo, installed by a script, or only
   documented? Current plan: shipped and documented, not auto-installed.
3. Is a Linux `usbmon` reader (D9) still wanted now that it is implemented and
   covered by synthetic tests? Confirm against a real Linux capture in the
   hardware session.
