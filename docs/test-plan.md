# Hardware test session

Everything below is scripted for the first time the amp is connected. Nothing
here writes to the amp until step 6, and step 1 gives you a full backup.

Prerequisites:

* amp powered on, USB cable that carries data (not a charge-only cable)
* Windows 11, Rust toolchain installed
* Fender FUSE closed (it grabs the HID interface)

Build once:

```console
cargo build --release
```

The binary is `target\release\scx2.exe`; below it is just called `scx2`.

## 1. Back up before touching anything

```console
scx2 dump -o backup-before.json
```

Expected output:

```text
connected: SuperChamp X2 Amp [1ed8:000e]
firmware build: May 30 2013
handshake done, requesting preset dump...
34 preset(s)
bank  idx  name                       amp               stomp  ...
...
saved backup-before.json
```

Then confirm the backup is complete and readable:

```console
scx2 show backup-before.json
scx2 show backup-before.json --preset 1:7 --raw
```

If `dump` fails, the error now lists the three usual causes (amp off, charge
cable, FUSE holding the port). `scx2 scan --all` shows whether Windows sees
`[1ed8:000e]` at all.

## 2. Compare the amp with the 2021 reference capture

The bundled capture came from a different amp, so differences are expected.
What matters is the shape, and that names/models resolve:

```console
scx2 replay captures\fuse-scx2-2021-06-07.pcap -o reference.json
scx2 diff reference.json backup-before.json
```

## 3. Map the parameter bytes

This is the step that unlocks real editing. In one terminal:

```console
scx2 learn
```

In the other, turn **one** knob on the amp, wait for the next poll, and note
what it prints:

```text
[2] 1 byte(s) changed
    amp bank=0 idx=0 offset 35: 0x00 -> 0x63
```

Record each knob here (see `docs/parameters.md` for the table). Do them one at
a time so the attributions stay unambiguous:

amplifier knobs: volume, gain, treble, bass, voice selector
effects knobs: fx type, fx level, reverb

Also useful: change a preset with the amp's own knob and watch the bank 0
blocks change as the live buffer reloads.

## 4. Write test (done, it works)

The amp needs the block frame **plus** the execute frame:

```console
scx2 apply amp --offset 44 --value 0x00
```

`apply` sends the modified block with direction byte `c3`, then `1c 03`.
Re-run `scx2 dump` and the value is there; power-cycling the amp restores the
stored preset, because this only writes the live buffer.

Confirmed on 2026-11-03: voice index 44 went `0x03` -> `0x00` -> `0x03` with
matching dumps. `--no-execute` reproduces the earlier "amp ignores it" case.

## 5. Capture a FUSE edit + save (optional but very useful)

If you still have a way to run FUSE, record USBPcap while you change a
parameter and save a preset. That gives us the store frame directly instead of
guessing:

```console
scx2 replay fuse-edit.pcap
scx2 replay fuse-edit.pcap --json > fuse-edit.json
scx2 diff backup-before.json fuse-edit.json
```

## 6. Restore a backup (only after step 4 works)

```console
:: shows all 172 reports and their frames, sends nothing
scx2 restore backup-before.json

:: sends them, c3 direction
scx2 restore backup-before.json --yes
```

Restore is the same mechanism as `apply`, just for every block. Do not run it
with `--yes` until `apply` has been confirmed, otherwise you will be writing
172 unverified frames into preset memory.

## What to send back

* the output of `scan` and `dump`
* the `learn` lines for each knob
* whether `apply --dir c3` (or `echo` / `c1`) changed the sound
* optionally, a FUSE edit/save capture
