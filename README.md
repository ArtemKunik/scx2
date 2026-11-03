# scx2

An open-source preset librarian for the **Fender Super Champ X2**, written in
Rust. No Silverlight, no dead installer, no 2013 runtime.

Fender FUSE is gone and the amplifier still speaks a simple 64-byte HID
protocol over USB. This project talks to it directly.

> **Status: read and write verified on hardware** (Super Champ X2, firmware
> `May 30 2013`). Dumping, polling and live parameter writes work; storing into
> a preset slot is still unknown. Writes stay dry-run unless you pass `--yes`.
> The decoder is also covered by tests against a real FUSE capture.
> See [docs/test-plan.md](docs/test-plan.md).

## Commands

Double-clicking `scx2.exe` (or running it with no arguments) opens the
desktop editor. Everything below is the same binary from a terminal.

```console
scx2 scan                       # list Fender HID devices
scx2 dump -o backup.json        # read all presets into a JSON backup
scx2 show backup.json           # print a backup
scx2 show backup.json --preset 1:7 --raw
scx2 diff a.json b.json         # byte-level comparison of two backups
scx2 learn                      # watch the live buffer, map knobs to offsets
scx2 monitor                    # print every report the amp sends
scx2 replay capture.pcap        # decode a USBPcap recording, no amp needed

# writes (dry run unless --yes)
scx2 apply amp --offset 35 --value 0x63 --from backup.json
scx2 send --hex "1c c1 05 00 00 00 01 01"
scx2 restore backup.json
```

`dump` and `replay` both print the full preset memory:

```text
34 preset(s)
bank  idx  name                       amp               stomp             mod             delay         reverb
--------------------------------------------------------------------------------------------------------------
live    0                             Clean Channel     None              None            Tape Delay    Small Hall
amp     0  voice                      '57 Champ         None              -               -             -
amp     1  Tweed Deluxe Clean         '57 Deluxe        None              -               -             -
amp     2  Tweed Deluxe Dirty         '57 Deluxe        Compressor        -               -             -
...
fx      6  Fender Spring Reverb       -                 -                 None            None          Spring '65
fx     11  Chorus + Reverb            -                 -                 Sine Chorus     None          Small Hall
```

Backups are lossless: every block keeps its raw 64-byte report, so a restore
is byte-for-byte and future versions can decode more from the same file.

## The editor

The window mirrors the amp: Volume, Gain, Treble, Bass and Voice, the five
knobs on the real front panel, plus Middle from FUSE, then the effect blocks. Dragging a knob changes
the value; the `x` on the knob (or a right-click) removes it again.

`+ knob` lists the front-panel set and, below it, the parameters only direct
control can reach (presence, master, bias, gate, cabinet, sag, bright...).
Those hidden ones are labelled as guesses with a hint, because Fender never
documented them. Every knob, hidden or not, can be removed.

Changes stay local until `Write to amp`, which sends the right frame for each
parameter (direct control for the amp knobs, block + execute for the rest).

## Build

Rust 1.75 or newer.

```console
cargo build --release
cargo test
target\release\scx2.exe scan
```

No amp is needed for `replay`; the bundled capture is a complete dump recorded
from FUSE in 2021.

## How it works

The amp is a composite HID device (`1ED8:000E`) with a vendor-defined HID
interface carrying 64-byte reports. The frame layout, the handshake, the three
banks and the model id tables are documented in
[docs/protocol.md](docs/protocol.md); the known parameters in
[docs/parameters.md](docs/parameters.md); the firmware that ships inside
FUSE 2.7.1 in [docs/firmware.md](docs/firmware.md); and what is inside that
firmware image in [docs/firmware-voices.md](docs/firmware-voices.md).

```
src/main.rs        entry point
src/cli/           scan, dump, replay, apply, param/probe, restore
src/protocol/      frames, message decoding, dump/preset model, diff, JSON
src/params.rs      direct-control frames, magic table, knob catalogue
src/capture.rs     USBPcap (.pcap) reader for offline work
src/models.rs      DSP model id -> name tables
src/transport.rs   hidapi device discovery and I/O
src/ui/            egui editor: app state, amp view, procedural materials
src/windows.rs     console detach and error dialog
```

## Roadmap

- [x] Enumerate the amp and read its identity
- [x] Handshake, request and decode a full dump
- [x] Offline verification against a real capture
- [x] Lossless JSON backup and reload
- [x] Preset/bank table, per-preset detail, byte-level diff
- [x] Live polling mode for parameter mapping (`learn`)
- [x] Raw frame send, single-byte apply, whole-backup restore (dry run first)
- [x] Confirm transport and write direction on hardware (block + `1c 03`)
- [x] Map the amp parameters through direct control (`param`, `probe`)
- [x] Desktop editor with real knobs and optional hidden controls
- [ ] Map the stomp/mod/delay/reverb offsets (`scx2 learn`)
- [ ] Store/rename presets (needs the save frame)
- [ ] Load/save Fender FUSE `.fuse` preset files

## Safety

Every write command prints the exact frame it would send and stops unless
`--yes` is given. `restore` additionally requires a full backup first (make
one with `dump`). Only the live buffer is ever written until the store path is
verified; a power cycle discards it.

## Credits

* The protocol capture comes from `rmduddy`, published on
  [snhirsch/mustang-midi-bridge issue #6](https://github.com/snhirsch/mustang-midi-bridge/issues/6).
* Model id tables and the Mustang framing reference:
  [snhirsch/mustang-midi-bridge](https://github.com/snhirsch/mustang-midi-bridge) (GPL-3.0),
  `docs/reference/` keeps the relevant source files for reference.
* Fender FUSE on modern Windows:
  [the wiki page](https://fender-mustang-amps-and-fuse.fandom.com/wiki/Fender_Fuse_on_Windows_11).

Unofficial project, not affiliated with Fender. Use it on hardware you own.
