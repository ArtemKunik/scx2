# Super Champ X2 USB protocol

Everything here was recovered from a real Fender FUSE session against a
Super Champ X2. The capture is checked in as
`captures/fuse-scx2-2021-06-07.pcap` (426 USBPcap packets, 198 HID reports);
every claim below can be re-derived from it with `scx2 replay`.

The capture was recorded by `rmduddy` in June 2021 and attached to
[issue #6 of snhirsch/mustang-midi-bridge](https://github.com/snhirsch/mustang-midi-bridge/issues/6).
It was made on Windows with FUSE 2.7.1 talking to an SCX2 at start-up: the
amp is enumerated, FUSE handshakes, then requests and receives the complete
preset memory.

## Device

| Field | Value |
| --- | --- |
| VID / PID | `0x1ED8` / `0x000E` |
| Product string | `SuperChamp X2 Amp` |
| Manufacturer | `FMIC` |
| Serial (example) | `05D5FF313537474257149022` |
| USB version | 1.10, full speed |
| Configuration | 1 config, 3 interfaces |

Interface 0 is a vendor-defined HID interface with two interrupt endpoints:

| Endpoint | Direction | Max packet | Function |
| --- | --- | --- | --- |
| `0x81` | IN | 64 | amp -> host reports |
| `0x01` | OUT | 64 | host -> amp commands |

The HID report descriptor is 32 bytes, i.e. a single unnumbered 64-byte
input/output report. Interface 2 is the USB audio recording interface, which
is why the amp also shows up as a sound card.

FUSE used `hidapi.dll` (see the wiki page about replacing it on Windows 11),
so we use hidapi as well: no libusb driver replacement, the Windows in-box HID
driver is enough.

## Session sequence

FUSE sends two handshake reports, then asks for the dump:

| # | Direction | Bytes 0.. | Meaning |
| --- | --- | --- | --- |
| 1 | host -> amp | `00 c3` | open the command channel |
| 2 | amp -> host | `00 00 00 00 "May 30 2013" ...` | firmware build banner |
| 3 | host -> amp | `1a c1` | select the v2 command dialect |
| 4 | amp -> host | `01 00 01 02 "May 30 2013" ...` | second banner |
| 5 | host -> amp | `ff c1` | request full parameter dump |
| 6 | amp -> host | `1f 01 00 24 ...` | current state summary |
| 7 | amp -> host | stream of `1c 01 ...` | one report per block, ~10 ms apart |
| 8 | amp -> host | `ff 01` | end of dump |

The dump is 198 reports and takes about two seconds. All reports are padded to
64 bytes with zeroes.

## Frame layout (amp -> host)

```text
offset  size  meaning
  0      1    0x1c                 frame marker
  1      1    0x01                 amp -> host
  2      1    kind                 see below
  3      1    bank                 0 live, 1 amp presets, 2 fx presets
  4      1    index                slot inside the bank
  5      1    -                    observed 0
  6      1    0x01                 always set
  7      1    flag                 1 on amp/mod/delay/reverb blocks, 0 on stomp
 16      2    model id             little endian, shared with the Mustang id space
 18      1    effect slot          stomp/mod/delay/reverb blocks only
 32     32    parameters           8-bit; 0x80 is the centre for bipolar values
```

Kind values seen in the capture:

| Kind | Block | Notes |
| --- | --- | --- |
| `0x00` | state | per-preset header, carries no payload |
| `0x04` | name | 32-byte ASCII name at offset 16 |
| `0x05` | amp | model + gain/EQ parameters |
| `0x06` | stomp | model + drive/level, effect slot at 18 |
| `0x07` | mod | chorus, tremolo, vibratone, ... |
| `0x08` | delay | model + time/feedback/mix |
| `0x09` | reverb | model + time/mix |
| `0x0A` | pedal | not preset scoped, `04` byte is not an index |
| `0x0D` | misc | not preset scoped in the live buffer |

## Banks

The amp reports three banks.

* **Bank 0, index 0** is the live edit buffer: what you hear right now. It
  carries every block (amp, stomp, mod, delay, reverb) plus an empty name.
* **Bank 1** is the amp side of the presets: 17 entries. Index 0 is named
  `voice` and holds the `'57 Champ` model; indices 1..16 are the 16 stored
  amp presets (`Tweed Deluxe Clean`, `'65 Twin`, ...). These blocks carry
  **amp and stomp only**.
* **Bank 2** is the effects side: 16 entries (`Slow Vibratone`, `300ms Tape
  Delay`, `Fender Spring Reverb`, ...). These carry **mod, delay and reverb**.

That split is verified: no bank 1 preset has a mod/delay/reverb block and no
bank 2 preset has an amp or stomp block. How the amp pairs the two banks when
you turn its preset knob is still unknown and is one of the first things to
check on hardware.

## Model ids

The ids are the same ones the Mustang family uses, which is why
`docs/reference/` contains the model tables from
[snhirsch/mustang-midi-bridge](https://github.com/snhirsch/mustang-midi-bridge)
(GPL-3.0, used here as reference material only). Every model reported by the
SCX2 resolved to a sensible name:

| Preset | Amp id | Name |
| --- | --- | --- |
| Tweed Deluxe Clean / Dirty | `0x0067` | `'57 Deluxe` |
| Tweed Bassman | `0x0064` | `'59 Bassman` |
| '65 Princeton Clean / Dirty | `0x006a` | `'65 Princeton` |
| '65 Deluxe | `0x0053` | `'65 Deluxe` |
| '65 Twin | `0x0075` | `'65 Twin` |
| 60's Brit | `0x0061` | `British '60s` |
| 70's Brit | `0x0079` | `British '70s` |
| 80's Brit | `0x005e` | `British '80s` |
| 90's Metal | `0x005d` | `American '90s` |
| 2000 Metal | `0x006d` | `Metal 2000` |
| JazzMaster | `0x00ea` | SCX2 only |
| Clean Channel | `0x00eb` | SCX2 only |
| voice | `0x007c` | `'57 Champ` |

Stomp `0x0007` is the compressor, `0x002d` vibratone, `0x0012` sine chorus,
`0x0041` sine tremolo, `0x002b` tape delay, `0x0016` mono delay, `0x0024`
small hall, `0x003a` large hall, `0x003b` large room, `0x000b` spring '65.

## Writing a block

Verified on hardware (2026-11-03, firmware `May 30 2013`): the amp accepts a
block it just reported if you send it back unchanged apart from the byte you
want to change, **followed by the execute frame**.

```text
host -> 1c c3 05 00 00 00 01 00 ... <modified block>   (direction byte c3)
host -> 1c 03 00 00 ...                                (execute)
```

Without the `1c 03` frame the amp reads the block and does nothing, which is
why the first attempts looked like a silent ignore. Do not use `0x01` (the
amp-to-host direction) or `0xc1` for a block write; only `0xc3` worked.

Test that proved it: the live buffer held a `'65 Deluxe` (voice index `0x03` at
offset 44). Sending offset 44 = `0x00` plus the execute frame, then re-dumping,
reported `0x00`. Writing `0x03` back restored it.

To change the voice, the model id at offset 16 has to move with the voice bytes.
Writing offsets 44..46 alone leaves the model where it was; writing both makes a
later dump report the new model (measured, `0x0053` to `0x006a`). Whether offset
50 should follow as well is untested.

Side effect: after an execute frame the amp reports byte 7 as `0x01` on every
amp block (it was `0x00` in the pre-write dump). It does not affect the data,
but `scx2 diff` will show it after any write session.

The UI and `scx2 apply` / `scx2 restore` send the execute frame automatically;
`--no-execute` disables it for experiments.

### Channel 1 is not a DSP target

Measured on hardware (2026-11-06, firmware `May 30 2013`): while the live amp
block reports `Clean Channel` (`0x00eb`, voice `0x00`), the amp acknowledges
block writes to the amp (`0x05`) and stomp (`0x06`) blocks but does not apply
them. A fresh dump still reports `0x00eb` and an empty stomp, and direct-control
parameter writes to the same model also report "no change". Mod, delay and
reverb blocks **do** apply on channel 1.

This is the amp's analogue clean channel, not a DSP voice. Channel 2 (the
front-panel **Channel** button or the footswitch) and one of its modelling
voices must be selected before a voice or stompbox will load. There is no host
frame for that switch in the captures; the editor detects `0x00eb` and explains
it instead of sending a write that the amp will ignore.

The USB live buffer carries the Channel 2 DSP state. The audible channel is
selected on the amp itself and is not reported in the captures, so the editor
cannot tell whether the player is hearing Channel 1 or Channel 2. If a voice
change is reported by the dump but not heard, the amp's **Channel** button is
the place to look.

## Bypassing an effect

Verified on hardware (2026-11-04): FUSE has a fast bypass frame that needs no
execute frame:

```text
host -> 19 c3 <family> <state> <slot>
```

`family` is the DSP kind minus 3: stomp `3`, mod `4`, delay `5`, reverb `6`.
`state` is `1` for bypassed and `0` for enabled; `slot` is the block's slot
byte (offset 18). Toggling the live reverb changed block offset 22 from `0` to
`1`, and toggling it back restored `0`. `scx2 bypass reverb 5 --off` sends it.

## Saving a preset

Verified on hardware (2026-11-04, firmware `May 30 2013`): the save frame is

```text
host -> 1c 01 03 <bank> <index> 00 01 01 <name at 16..48>
```

The amp writes its **current live buffer** into `<bank>/<index>` and stores
`<name>` (32 bytes, NUL padded). There is no per-block payload.

* `bank` 1 is the amp preset bank, `2` the FX bank. FUSE writes `0` here because
  a Mustang has one flat preset list; the SCX2 uses the bank byte.
* Verified by saving into `fx 15` (empty, `NONE`) and `amp 15`, then reading both
  slots back with `scx2 dump`. The names and contents changed as expected.
* A stored slot also accepts a direct block write (`1c c3 ...` + `1c 03`); that
  is how `scx2 restore` put the original bytes back after the test.
* Persistence is proven: the `fx 15` marker still read back as `PERSIST TEST`
  after a full power cycle, so the save reaches flash rather than RAM.

## Recalling a preset

There is **no host recall** on the SCX2. FUSE's load frame

```text
1c 01 01 <bank> <slot> 00 01
```

makes the amp stream its current state (a read); type bytes `0x00`, `0x02` and
`0x04` get no reply. Writing a preset's blocks to bank 0 is not a substitute:
on this amp that edits the currently selected preset slot, which can overwrite
stored memory. The front-panel preset knob is the only way to recall.

Caution: after a recall attempt the amp's "current slot" pointer can move, so
later bank-0 block writes may land in that slot. `scx2 recall` is therefore
read-only: it prints the slot and its blocks.

## Reports the amp sends by itself

Captured on hardware (2026-11-03, firmware `May 30 2013`) with `scx2 monitor`,
without asking the amp for anything. Turning a front-panel knob streams one
report per step:

```text
05 01 02 53 00 00 00 0c 00 2d 2d    volume is now 0x2d
05 01 02 53 00 01 01 0c 00 fe fe    gain is now 0xfe
```

| Offset | Meaning |
| --- | --- |
| 2 | `0x02` for an amp parameter |
| 3..5 | current amp model id, little endian |
| 5, 6 | parameter index (direct-control numbering), repeated |
| 7 | selector, `0x0c` for amp parameters |
| 9..11 | value, 16-bit little endian; `round(value / 257)` is the block byte |

This is the direct-control frame with direction byte `0x01` instead of `0xc3`.
Only volume and gain have been seen this way so far. The field names match
[OpenFUSE](https://github.com/dbarbashov/openfuse)'s reading of FUSE 2.7.1, which
is where the `KnobPosition` meaning of `1d` below comes from.

Other reports:

| Report | Meaning | Status |
| --- | --- | --- |
| `00 00 05 <status>` | answer to a direct-control write, about 1 ms later; 0 means accepted. The value is not echoed | measured |
| `05 01 <module> <model lo> <model hi> <index> <index> 01 00 <value lo> <value hi>` | one effect parameter; `<module>` was 5 for delay and 6 for reverb, `<index>` is block offset `32 + index`, and `round(value / 257)` is the byte | decoded from the capture |
| `1d 01 <index> <position>` | selector knob position. `index` 1 is the Voice knob, 2 the FX-select knob. `position` is the 4-bit Grey code of the 1-based voice position: `0x00` Clean Channel, `0x03` '57 Deluxe, `0x06` '65 Deluxe, `0x0a` British '70s. | sent at the end of a dump; whether they also arrive when the knob turns is untested |
| `1f 01 00 <n> ...` | state summary before the block stream; the payload after the marker is not decoded | marker known, payload unknown |

`ff c5`, which FUSE uses to ask for the current preset only, returns the full
dump on this amp (195 reports, about 1.9 s), so a dump is always the slow path.
The live buffer does come first, within the first few reports.

## Still unknown

* **Meaning of the parameter bytes.** Offsets 32+ hold the amp parameters in
  some order (gain, volume, treble, bass, ...). Map them by turning one knob at
  a time while `scx2 learn` is running.
* **The payload of the `1f 01` state summary.** The marker and its position
  before a dump are known, but the bytes after it are not decoded. The
  `05 01 <module>` effect values and `1d` selector positions are now decoded;
  see [Reports the amp sends by itself](#reports-the-amp-sends-by-itself).

Storing and renaming are no longer unknown: see
[Saving a preset](#saving-a-preset). Renaming a stored slot was verified with
both `scx2 store --name` and a direct `0x04` name block.

## Recording your own capture

On Windows, with [USBPcap](https://desowin.org/usbpcap/) (it comes with
Wireshark):

1. Start a capture on the root hub the amp is attached to.
2. Open FUSE, connect, change one thing, save, quit.
3. Stop the capture and keep the `.pcap`.

On Linux, with `usbmon`:

```console
sudo modprobe usbmon
lsusb -d 1ed8:000e                 # note the bus, e.g. "Bus 001"
sudo tshark -D                     # pick the matching usbmonN
sudo tshark -i usbmon1 -w scx2-linux.pcap
```

Turn a knob or run `scx2 monitor` while it records, then stop with Ctrl-C.
Keep it short: `usbmon` records every transfer on the bus.

Both formats replay: link type 249 (USBPcap) and link type 220 (Linux
`usbmon` mmapped). `scx2 replay your-capture.pcap` decodes the amp's side
offline, and `scx2 replay --json` gives you the raw bytes for diffing two
captures.
