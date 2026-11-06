# scx2

An open-source preset librarian for the **Fender Super Champ X2**, written in
Rust. No Silverlight, no dead installer, no 2013 runtime.

Fender FUSE is gone and the amplifier still speaks a simple 64-byte HID
protocol over USB. This project talks to it directly.

> **Status: read and write verified on hardware** (Super Champ X2, firmware
> `May 30 2013`). Dumping, polling, live parameter writes, effect editing,
> `.fuse` import/export, **store**, **rename** and **restore** work, and a saved
> slot survives a power cycle. Store and rename also work from the editor, which
> reads the slot back after each write; a restore re-dumps and compares every
> slot with the file. Recall is read-only: the SCX2 has no host preset-load
> command, so the front-panel knob is the only way to load a slot. Writes stay
> dry-run unless you pass `--yes`. The decoder is also covered by tests against
> a real FUSE capture.
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
scx2 bypass reverb 5 --off                         # verified on hardware
scx2 recall 7 --bank 1                             # read-only: prints the slot
scx2 midi --list                                   # list MIDI inputs
scx2 midi --yes                                    # headless foot-controller bridge
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

The four effect slots are editable too: click any slot to open a focused editor
for that pedal. The pedal is drawn large, and every parameter gets its own
control in a rack beside it, with the FUSE name above and the 0–10 value below.
Drag a control or scroll over it; hold Shift for fine control and double-click
to reset one control. The model list is one click away, and right-clicking a
slot (or hovering and pressing the `x`) clears it. A loaded effect has an
**ON/OFF** pill in its slot header: it bypasses the effect without unloading
it, so the model and every setting stay exactly where they were. The knob names
come from FUSE's own model table (Level, Time, Fdbk, Depth, Mix and so on), and
loading a model gives it that model's default parameter bytes instead of
inheriting the previous effect's settings.

Open **Effect Parameters** for the raw byte view of every effect block: all 32
parameters, with the FUSE name for the slots the model uses. Each loaded
effect also has a **Bypass** toggle (offset 22, FUSE's `BypassState`, verified
on hardware). The names and defaults come from FUSE's model table and are not
yet verified by ear, so the panel says so; `scx2 learn` is the way to confirm
them.

Every knob and parameter cell is shown on a **0–10 scale**, like the amp's
front panel; the full 0–255 byte is still what gets sent to the amp.

The **Voice** knob and the model plate on the grille open the **Amp & Speaker**
picker: a head-and-cab picture of the current rig, the fourteen SCX2 voices on
the left and FUSE's cabinet list on the right. Picking a voice or a speaker
updates the picture and the live buffer immediately, and the picker stays open
so both can be changed in one visit. Dragging the Voice knob still sweeps
through the voices.

The toolbar has **Save to Amp…**, **Open…**, **Save As…**, **Save Backup**,
**Restore…**, **Undo**, **Redo** and **Revert**, with Ctrl+O / Ctrl+S / Ctrl+Z /
Ctrl+Shift+Z shortcuts. A `●` in the title means the current dump has unsaved
changes. **Save to Amp…** writes the live buffer into a bank/slot with FUSE's
verified save frame: it shows the name the amp will store, warns when it is
about to replace a named slot, offers to export that slot as a `.fuse` first,
and reads the slot back afterwards, comparing every payload byte and the name.
Right-click a stored preset in the sidebar to **Rename…** it: only the `0x04`
name block and an execute frame are written, and the read-back checks that no
other byte moved.
**Restore…** writes a JSON backup back to the amp. It lists every slot it will
replace, saves the current amp state to a backup first, then writes block by
block with a progress bar and a cancel that stops between blocks; afterwards it
re-dumps and compares every restored slot with the backup. The backup's live
buffer is never written.
`Open` and the drag target accept both JSON backups and Fender FUSE `.fuse`
files; `Save As` writes JSON for the whole dump, or `.fuse` for the selected
preset.
**Import…** in the sidebar's Live header copies selected modules — amplifier,
stomp, modulation, delay, reverb — from a `.fuse` preset or a backup into the
live buffer. Only the ticked modules move; every other block stays
byte-identical, and an unknown model is shown and left unticked until you
choose it.
**Save effects…**, **Save voice…** and **Save CH1…** in the sidebar headers are
FUSE's sub-preset dialogs: bank 2 (mod/delay/reverb), bank 1 voices 0–15
(amp/cabinet/stomp), and the Clean Channel slot (amp 16, only when channel 1 is
selected). They share the Save-to-Amp dialog and read-back, which compares only
the blocks the target bank stores.

**Tuner** opens a polyphonic guitar tuner fed by the amp's USB audio output.
Click the button in the toolbar and strum: all six strings of standard tuning are
tracked at once, each with its own cents readout, and the note under the cursor
gets a large strobe display. A string turns green when it is within a few cents.
The tuner follows the amp's input automatically; **Rescan** re-lists devices, and
if Windows has the amp's capture endpoint disabled, **Enable amp input** switches
it on from inside the app (the same operation as the Sound control panel's
Enable item). On Linux the input appears on its own, so **Rescan** is enough.
The USB signal is post-amp and post-effects, so a clean voice with
effects bypassed gives the most reliable reading.

**Recorder** writes one input channel to a 24-bit WAV in
`Music/SCX2 Recordings`, with a `.json` sidecar holding the live preset
(voice, cabinet, every block and its raw bytes) so a take can be recalled.
Pick the input and channel, then **Record** / **Stop**. The meter latches the
take's peak and flags clipping; **Open folder** shows the takes. The full
capture spec is in [docs/recording.md](docs/recording.md).

**MIDI** opens the foot-controller bridge. The amp has no MIDI input, so the
controller talks to `scx2`, which sends the same verified bypass frame as the
ON/OFF pill. Pick the MIDI input, press **Learn** on Stomp, Mod, Delay or
Reverb, and move the control you want to use; CC, note and program-change
messages are all accepted. **Toggle** flips the effect on each press;
**Follow** switches it on while the control is on, which also suits a latching
footswitch that sends 127/0. The mappings are saved in `midi.json` under the
platform config directory. `scx2 midi --yes` runs the same bridge without the
editor, which is the useful mode for a laptop or small box left connected to
the amp; `scx2 midi --list` prints the available inputs.

MIDI on/off edits the live buffer. Recalling a preset on the amp's front panel
restores that preset's stored effect states, and preset recall itself still has
no host command. A computer must be running `scx2` between the controller and
the amp.

For **Neural Amp Modeler / IR re-amping**, record the dry guitar: select the
hi-Z instrument input of an audio interface and the channel the guitar is on.
The SCX2 cannot provide that signal — its USB is capture-only (it has no
playback endpoint), and the USB output is post-effects and post-cabinet, so
**Auto (amp USB)** records a finished amp sound, not a DI. A DI box with a
through output, or a splitter before the amp, lets you play through the amp
while capturing the dry signal. The sidecar records the source channel so a
take's origin is unambiguous.

The `+` on the panel lists the front-panel set and, below it, the parameters only direct
control can reach (presence, resonance, master, bias, gate, sag, bright...).
FUSE's own Super Champ X2 model table names most of them; the few that are
still guesses say so in their hint. **Cabinet** is not a knob: it is chosen
together with the voice in the amp & speaker picker. The menu's **Device**
section holds **USB Gain…**, the amp's USB recording level (raw 0–255, shown
with its percentage): a device setting, written on its own and read back from
the misc block. Every knob, hidden or not,
can be removed.

The knobs stay in sync with the amp. Turning a knob on the amp moves it on
screen, and dragging one on the Live Buffer changes the sound straight away, so
there is no write button. Stored presets are read-only in place: edit the live
buffer, then use **Save to Amp…** to write the slot it should live in. Without
an amp you can edit a backup and save it.

## Build

Rust 1.75 or newer.

```console
cargo build --release
cargo test
target/release/scx2 scan
```

No amp is needed for `replay`; the bundled capture is a complete dump recorded
from FUSE in 2021.

### Linux

The editor and every command build and run on Linux. The only extra system
package is ALSA's development files, which `cpal` links against for the tuner
and recorder:

```console
sudo apt install libasound2-dev     # Debian, Ubuntu, Pop!_OS
sudo dnf install alsa-lib-devel     # Fedora
sudo pacman -S alsa-lib             # Arch
```

`hidapi` uses its pure-Rust `basic-udev` backend on Linux, so `libudev-dev` is
not needed. The window runs on X11 or Wayland through eframe/winit, and the file
dialogs use `xdg-desktop-portal`, which desktop installs already run.

The amp's raw HID node (`/dev/hidraw*`) is root-only until udev is told
otherwise. Install the rule once, then replug the amp:

```console
sudo packaging/udev/install.sh
scx2 scan
```

The script installs `70-scx2.rules`, removes the obsolete `99-scx2.rules` that
older revisions used, reloads udev and applies the ACL to the current nodes.
The manual equivalent is:

```console
sudo cp packaging/udev/70-scx2.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --action=add --subsystem-match=hidraw
scx2 scan
```

The rule number matters. `uaccess` is applied by `73-seat-late.rules`, so a
rule numbered `99` adds the tag too late on a normal plug; that is why the old
`99-scx2.rules` only worked after a manual `--action=add`. Keep the rule below
73 (`70-scx2.rules` is the shipped name).

Use `--action=add`: a plain `udevadm trigger` sends `change` events, and udev
deliberately keeps the existing permissions for those, so the ACL never appears.
Replugging the amp works too. The editor also retries a permission-denied open
for two seconds, so starting it in the same moment as the plug does not drop it
into the demo capture.

If `scx2 scan --all` lists nothing at all, the machine is not exposing any
`/dev/hidraw*` nodes (`ls /dev/hidraw*`) — common in VMs without USB
passthrough, and unrelated to the app.

The Windows-only **Enable amp input** action is hidden on Linux: ALSA and
PipeWire publish the amp's USB capture device as soon as it is plugged in, so
**Rescan** in the tuner is enough.

[docs/linux.md](docs/linux.md) is the full delivery spec: package lists per
distribution, the hardware checks, and the CI definitions.

## How it works

The amp is a composite HID device (`1ED8:000E`) with a vendor-defined HID
interface carrying 64-byte reports. The frame layout, the handshake, the three
banks and the model id tables are documented in
[docs/protocol.md](docs/protocol.md); the known parameters in
[docs/parameters.md](docs/parameters.md); the `.fuse` XML format in
[docs/fuse.md](docs/fuse.md); the firmware that ships inside FUSE 2.7.1 in
[docs/firmware.md](docs/firmware.md); what is inside that firmware image in
[docs/firmware-voices.md](docs/firmware-voices.md); which FUSE features are
still missing in [docs/fuse-coverage.md](docs/fuse-coverage.md); and the
implementation plan in [docs/roadmap.md](docs/roadmap.md).

```
src/main.rs        entry point
src/cli/           scan, dump, replay, apply, param/probe, restore
src/protocol/      frames, message decoding, dump/preset model, diff, JSON
src/params.rs      direct-control frames, magic table, knob catalogue
src/capture.rs     USBPcap (.pcap) reader for offline work
src/fuse.rs        Fender FUSE `.fuse` XML import/export
src/models.rs      DSP model id -> name tables, effect params/defaults, FUSE ids
src/transport.rs   hidapi device discovery and I/O
src/tuner.rs       USB audio capture and YIN pitch detection
src/recorder.rs    24-bit WAV takes with a preset sidecar
src/endpoint.rs    Windows capture-endpoint switch (absent elsewhere)
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
- [x] Confirm stored presets survive a power cycle

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
* [midir](https://github.com/Boddlnagg/midir) (MIT) provides the MIDI input for
  the foot-controller bridge.
* [OpenFUSE](https://github.com/dbarbashov/openfuse) (MIT) pointed to the meaning
  of the `0x05` and `0x1d` reports and the write acknowledgement; they were then
  checked on real hardware where noted in `docs/protocol.md`.
* Fender FUSE on modern Windows:
  [the wiki page](https://fender-mustang-amps-and-fuse.fandom.com/wiki/Fender_Fuse_on_Windows_11).

Unofficial project, not affiliated with Fender. Use it on hardware you own.
