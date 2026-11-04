# scx2

An open-source preset librarian for the **Fender Super Champ X2**, written in
Rust. No Silverlight, no dead installer, no 2013 runtime.

Fender FUSE is gone and the amplifier still speaks a simple 64-byte HID
protocol over USB. This project talks to it directly.

> **Status: read and write verified on hardware** (Super Champ X2, firmware
> `May 30 2013`). Dumping, polling, live parameter writes, effect editing,
> `.fuse` import/export, **store** and **rename** work. Recall is read-only:
> the SCX2 has no host preset-load command, so the front-panel knob is the only
> way to load a slot. Writes stay dry-run unless you pass `--yes`. The decoder
> is also covered by tests against a real FUSE capture.
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
scx2 import song.fuse -o song.json   # import a Fender FUSE preset
scx2 export backup.json --preset 1:7 -o song.fuse

# writes (dry run unless --yes)
scx2 apply amp --offset 35 --value 0x63 --from backup.json
scx2 send --hex "1c c1 05 00 00 00 01 01"
scx2 restore backup.json
scx2 store --bank 1 --index 7 --name "My preset"   # verified on hardware
scx2 rename "My preset" --bank 1 --index 7         # verified on hardware
scx2 recall 7 --bank 1                             # read-only: prints the slot
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

Everything is drawn, not shipped as bitmaps: the cabinet follows the selected
voice's period (tweed, brownface, blackface, silverface, British, modern or
metal), and each effect model gets its own hardware face - a stompbox, wah
treadle, tape echo, spring reverb tank or rack processor - with knobs that sit
where the parameter bytes put them.

The four effect slots are editable too: click a slot to load or change its
effect, drag the knobs drawn on the pedal to change that effect's parameters,
and right-click (or hover and press the `x`) to clear it. The knob names now
come from FUSE's own model table (Level, Time, Fdbk, Depth, Mix and so on), and
loading a model gives it that model's default parameter bytes instead of
inheriting the previous effect's settings.

Open **Effect Parameters** for the complete editor: all 32 bytes of every
effect block, with the FUSE name for the slots the model uses. Each loaded
effect also has a **Bypass** toggle (offset 22, FUSE's `BypassState`). The
names, defaults and bypass byte are not yet verified on SCX2 hardware, so the
panel says so; `scx2 learn` is still the way to confirm them.

The toolbar has **Open…**, **Save As…**, **Save Backup**, **Undo**, **Redo**
and **Revert**, with Ctrl+O / Ctrl+S / Ctrl+Z / Ctrl+Shift+Z shortcuts. A `●`
in the title means the current dump has unsaved changes. `Open` and the drag
target accept both JSON backups and Fender FUSE `.fuse` files; `Save As` writes
JSON for the whole dump, or `.fuse` for the selected preset.

The `+` on the panel lists the front-panel set and, below it, the parameters only direct
control can reach (presence, master, bias, gate, cabinet, sag, bright...).
Those hidden ones are labelled as guesses with a hint, because Fender never
documented them. Every knob, hidden or not, can be removed.

The knobs stay in sync with the amp. Turning a knob on the amp moves it on
screen, and dragging one on the Live Buffer changes the sound straight away, so
there is no write button. Stored presets are read-only while connected, because
the command that saves to a slot is still unknown. Without an amp you can edit
a backup and save it.

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
[docs/parameters.md](docs/parameters.md); the `.fuse` XML format in
[docs/fuse.md](docs/fuse.md); the firmware that ships inside FUSE 2.7.1 in
[docs/firmware.md](docs/firmware.md); and what is inside that firmware image in
[docs/firmware-voices.md](docs/firmware-voices.md).

```
src/main.rs        entry point
src/cli/           scan, dump, replay, apply, param/probe, restore
src/protocol/      frames, message decoding, dump/preset model, diff, JSON
src/params.rs      direct-control frames, magic table, knob catalogue
src/capture.rs     USBPcap (.pcap) reader for offline work
src/fuse.rs        Fender FUSE `.fuse` XML import/export
src/models.rs      DSP model id -> name tables, effect params/defaults, FUSE ids
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
- [x] Add FUSE's effect parameter names and per-model defaults
- [ ] Verify the effect map and defaults on SCX2 hardware (`scx2 learn`)
- [x] Store and rename presets (save frame verified on hardware)
- [x] Determine how the SCX2 recalls a preset (there is no host command)
- [x] Load/save Fender FUSE `.fuse` preset files
- [ ] Confirm stored presets survive a power cycle

## Safety

Every write command prints the exact frame it would send and stops unless
`--yes` is given. `restore` additionally requires a full backup first (make
one with `dump`). `store` writes the live buffer into a stored slot, so keep a
backup; `rename` writes only the `0x04` name block. Recall is read-only because
the SCX2 has no host preset-load command.

## Credits

* The protocol capture comes from `rmduddy`, published on
  [snhirsch/mustang-midi-bridge issue #6](https://github.com/snhirsch/mustang-midi-bridge/issues/6).
* Model id tables and the Mustang framing reference:
  [snhirsch/mustang-midi-bridge](https://github.com/snhirsch/mustang-midi-bridge) (GPL-3.0),
  `docs/reference/` keeps the relevant source files for reference.
* Effect parameter names, per-model defaults and the `.fuse` XML shape:
  [jtangelder/refuse](https://github.com/jtangelder/refuse), used as reference
  data for FUSE's own model definitions.
* [OpenFUSE](https://github.com/dbarbashov/openfuse) (MIT) pointed to the meaning
  of the `0x05` and `0x1d` reports and the write acknowledgement; they were then
  checked on real hardware where noted in `docs/protocol.md`.
* Fender FUSE on modern Windows:
  [the wiki page](https://fender-mustang-amps-and-fuse.fandom.com/wiki/Fender_Fuse_on_Windows_11).

Unofficial project, not affiliated with Fender. Use it on hardware you own.
