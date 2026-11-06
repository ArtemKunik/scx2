# Hardware test session

Everything below is scripted for the first time the amp is connected. Nothing
here writes to the amp until step 6, and step 1 gives you a full backup.

Prerequisites:

* amp powered on, USB cable that carries data (not a charge-only cable)
* Windows 11, or Linux (Ubuntu 24.04 is the reference target; see
  [linux.md](linux.md))
* Rust toolchain installed
* Fender FUSE closed (it grabs the HID interface)

Build once:

```console
cargo build --release
```

The binary is `target\release\scx2.exe` on Windows and `target/release/scx2`
on Linux; below it is just called `scx2`.

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

## 7. Verify the new effect map and the save path

The effect parameter names/defaults now come from FUSE's model table. They are
consistent with the capture but not yet proven on the SCX2:

1. Load a known effect (for example Overdrive) into the live buffer.
2. Open **Effect Parameters** and compare the labels with FUSE's editor.
3. Turn one knob at a time and run `scx2 learn`; record which offset moves.
4. Repeat for a delay (Time, Fdbk, Flutter) and a reverb (Level, Decay, Dwell).

### Save and rename: verified 2026-11-04

The save frame was confirmed on hardware against `fx 15` (empty, `NONE`) and
`amp 15`, then both slots were restored from the pre-write backup:

```console
scx2 dump -o before-store.json
scx2 store --bank 2 --index 15 --name "SCX2 TEST"          # prints the frame
scx2 store --bank 2 --index 15 --name "SCX2 TEST" --yes
scx2 dump -o after-store.json                              # fx 15 renamed
scx2 rename "Another name" --bank 2 --index 15 --yes       # name block only
```

The save frame is `1c 01 03 <bank> <index> 00 01 01 <name>`. A stored slot also
accepts a direct block write (`1c c3 ...` + `1c 03`), which is how the original
bytes were put back.

### Recall: no host command

The SCX2 treats FUSE's `1c 01 01 <bank> <slot> 00 01` as a state read; type
bytes `0x00`, `0x02` and `0x04` get no reply. The front-panel preset knob is the
only way to load a slot. Do **not** write a preset's blocks to bank 0 to fake a
recall: on this amp that edits the currently selected preset slot and can
overwrite stored memory. `scx2 recall` is read-only for that reason.

### Persistence: verified 2026-11-04

A marker was saved into `fx 15`, the amp was powered off and back on, and
`scx2 dump` still reported `fx 15` as `PERSIST TEST`. The save path therefore
reaches flash, not just RAM. The slot was then restored to `NONE`.

`.fuse` round-trips are offline and already covered by `cargo test`; a useful
hardware check is to import a FUSE preset, send it to the live buffer with the
editor, and confirm the amp reports the same models and parameter bytes.

### Save from the editor (M1.1)

The frame is the verified one; this checklist covers the dialog around it.

1. With the Live Buffer selected, open **Save to Amp…**. The dialog defaults to
   the first empty amp slot (the voice slot 0 is not offered), and the slot
   grid shows every stored name.
2. Pick a named slot: **Replaces** must name it and the backup box must be
   ticked. Export the backup and confirm the `.fuse` file opens in the editor.
3. Save. The status bar must read `read-back matches`, the sidebar must select
   the written slot, and its name must be the one in the dialog.
4. Change the volume, save to the same slot again, and confirm the read-back
   matches and the slot shows the new value.
5. Type a name longer than 32 bytes: **Trim to 32** must appear and the Save
   button must stay disabled until the name fits.
6. Disconnect the amp, open the dialog again, and confirm the status bar shows
   the dry-run frame and nothing is written.
7. Power-cycle and confirm the slot survived.

### Rename from the editor (M1.2)

1. Right-click a named preset in the sidebar and choose **Rename…**. The dialog
   must show `Amp preset N · now "old name"`.
2. Type a new name and press Rename. The status bar must read
   `Renamed amp N to "…" · other bytes unchanged` and the sidebar must show the
   new name.
3. Open the slot's stored state and confirm the voice and every effect are
   unchanged (the read-back already compares the payload).
4. Clear the name field: Rename must stay disabled.
5. Paste more than 32 bytes: **Trim to 32** must appear and Rename must stay
   disabled until the name fits.
6. Disconnect and repeat: both frames must appear in the status bar as a dry
   run, and nothing is written.

### Restore from the editor (M1.3)

1. Open **Restore…** and pick a backup. The dialog must show the source device,
   the amp/FX preset counts and every target slot under **Will write**.
2. Try each scope: all stored, amp only, FX only, and one preset from the
   combo box. The block and slot counts must follow the choice.
3. Confirm. The **Back up and restore** button must open a file dialog first;
   cancelling it must leave the amp untouched.
4. Watch the progress bar: blocks must advance, and **Cancel restore** must stop
   between blocks. The status must name how many blocks were written and list
   the slots that no longer match.
5. Restore a full backup and let it finish. The status must read
   `re-dump matches the backup`, and the amp's front-panel recall must load the
   restored presets with the right names.
6. Restore only the FX bank and confirm the amp presets are unchanged.
7. Power-cycle and confirm the restored slots survived.

### Import modules from a preset (M3.1)

1. On the Live Buffer, click **Import…** in the sidebar's Live header. Pick a
   `.fuse` preset, then a JSON backup with several presets: the source combo
   must list every preset in the file.
2. Tick only **Reverb** and import. The status bar must name the reverb, and a
   `scx2 dump` before/after must show only the reverb block changed.
3. Import only **Amplifier**: the voice, cabinet and advanced amp must follow,
   while stomp/mod/delay/reverb stay byte-identical.
4. Pick a source preset with no stomp and tick **Stomp**: the live stomp must
   be cleared.
5. Open a file with an unknown model: the row must say "unknown model" and be
   unticked; ticking it must copy the raw id.
6. Undo the import: every block must return to its previous bytes.
7. Repeat offline: the import must apply to the local live buffer and survive
   **Save As**.

### Save effects / voice / CH1 (M3.2)

1. With effects loaded on the Live Buffer, click **Save effects…** in the FX
   Presets header. The bank must be fixed to FX, the grid limited to slots
   0–15, and the dialog must name the modules it writes (mod, delay, reverb).
2. Save to a slot, let the read-back finish, then `scx2 dump` and confirm the
   amp bank is byte-identical to before.
3. Click **Save voice…** in the Amp Presets header. The grid must stop at slot
   15 (slot 16 is the clean channel). Save, then confirm the FX bank is
   unchanged.
4. Switch the amp to channel 2 and open **Save CH1…**: the dialog must warn and
   leave Save disabled. Switch to channel 1: the target must be amp 16
   (`Clean Channel`) and Save must enable.
5. Save CH1, let the read-back finish, and confirm the clean channel survives a
   power cycle.
6. Offline, all three dialogs must print their frames as dry runs.

### USB Gain (M4.1)

1. Connect the amp and open **Parameters → + → Device → USB Gain…**. The
   dialog must show the value the amp reports.
2. Set a distinct value (say 64) and write. The status must read
   `USB gain 64 (25%) · read-back matches`.
3. Re-dump and confirm the misc block still holds that value, with every other
   payload byte unchanged.
4. Record over USB at two gain settings: the capture level must follow.
5. Reconnect or power-cycle: the value must still be there.
6. Disconnect and repeat: the status must show the dry-run frames, and nothing
   is written.
7. If the amp ignores the block write, capture the FUSE interaction and adjust
   the direction/execute bytes before shipping.

### Preset info (M4.2)

1. Right-click a stored preset → **Preset Info…**. The header must show the
   device name and slot read-only.
2. Fill description, author, tags (comma separated), genre, both sub-genres, a
   rating and comments. Save: the title dot must appear.
3. **Save As** a JSON backup and reopen it. Every field must come back, and the
   device blocks must be byte-identical to before.
4. Save As a `.fuse` and reopen it: the device name and sound survive, but the
   info is not part of `.fuse`; the dot stays until the info is saved in a
   backup.
5. Undo after saving info: the previous fields must come back.

### MIDI foot controller (M8.1)

1. Connect a class-compliant MIDI controller, open **MIDI**, and confirm it is
   in the input list after **Refresh**.
2. Learn a CC for Stomp, Mod, Delay and Reverb. Toggle each one: the pill on
   the pedal and the note in the status bar must follow, and the sound must
   change without unloading the model or moving its knobs.
3. Repeat with a note-on/off control and with a program-change-only control.
4. Set a channel filter and confirm another channel is ignored.
5. Switch one effect to **Follow** and use a latching footswitch that sends
   127/0: the effect must be on in one position and off in the other.
6. Recall a stored preset on the amp's front panel. The app should report the
   preset's effect states, and the next MIDI press must target the live buffer's
   slot, not the previous preset's slot.
7. Restart the editor: the mappings must still be there.
8. Run `scx2 midi --list`, then `scx2 midi --yes` with the editor closed. Each
   press must print the effect and state, and the amp must respond the same way.
9. Without `--yes`, the bridge must print dry-run frames and change nothing.

## What to send back

* the output of `scan` and `dump`
* the `learn` lines for each knob
* whether `apply --dir c3` (or `echo` / `c1`) changed the sound
* optionally, a FUSE edit/save capture
