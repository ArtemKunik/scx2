# scx2 roadmap and feature specs

This is the implementation spec for closing the gap between `scx2` and Fender
FUSE 2.7.1. It covers the SCX2 features we can build, the features that need
more protocol work, and the FUSE features that belong to other products.

The coverage snapshot is in [fuse-coverage.md](fuse-coverage.md). This document
is the plan: scope, protocol facts, UI shape, safety rules, acceptance criteria
and ordering.

## 1. Scope and principles

**Target:** Super Champ X2, firmware `May 30 2013`, FUSE 2.7.1 files.

**Not a target:** Mustang, Mustang Floor, G-DEC, Bronco and Passport Mini
editors. Some of their UI and file blocks are documented here only so we do not
mistake them for SCX2 features.

**Principles**

* Read-only by default. Every device write is a dry run unless `--yes` (CLI) or
  an explicit confirmation (editor).
* Never overwrite a stored slot without first offering to save its current
  contents to a backup.
* Verify by reading back. A write is not "done" until a fresh dump reports the
  expected bytes.
* Preserve unknown data. `.fuse` files, backups and device blocks may contain
  fields we do not decode yet; round trips must not silently drop them.
* Keep the CLI and editor on the same library code. The CLI is the reference
  implementation and the editor is a front end.
* Prefer safe, reversible operations. Firmware flashing and factory reset are
  last, not first.

## 2. Milestones

| Milestone | Theme | Main features |
| --- | --- | --- |
| M0 | Current | Read/dump, live edit, effects, voice, cabinet, `.fuse`, backup, store/rename/bypass CLI, PC-audio tuner |
| M1 | Device operations in the editor | Save to amp, rename, restore |
| M2 | Preset library | New/duplicate/delete, PC/Amp browser, search/sort/filter |
| M3 | Partial data | Module import, SAVE EFFECTS, SAVE VOICE, SAVE CH1 |
| M4 | Missing controls and metadata | USB Gain, preset info (description/tags/genres/author) |
| M5 | High-risk device work | Factory restore, firmware update, recovery path |
| M6 | Amp-native tuner | Tuner/mute frames, fallback behaviour |
| M7 | Optional/other products | Media library, expression pedal, FX loop, SD card, phrase sampler, community |

Effort is S (hours), M (a day or two), L (multi-day, needs hardware sessions).

## 3. Cross-cutting requirements

### 3.1 Safety

* All write commands take `--yes`; the editor uses an explicit modal.
* Before overwriting a stored slot, read it and offer "Save current slot to
  backup" in the same dialog.
* Before restore/factory restore/firmware update, require a fresh backup.
* Show the exact bytes/frames in a details view (existing `send --hex` style).
* A cancelled restore stops between blocks and reports what was already
  written; it does not pretend to roll back.
* Firmware update is the only operation that can leave the amp unbootable.
  It is gated behind a separate flag and a boot-mode detection check.

### 3.2 Verification

* After a slot write: read the slot back and compare every block byte except
  the known `byte 7 = 0x01` execute side effect.
* After a restore: re-dump and show a diff, including the expected execute
  flag differences.
* After USB Gain: read the `0x0D` misc block back.
* Hardware-only claims are marked in docs and tests cannot replace a power
  cycle.

### 3.3 Common UI

* One shared "device operation" dialog with:
  - operation title and target (slot / bank / whole amp),
  - current and new name,
  - overwrite warning,
  - backup offer,
  - progress with cancel,
  - result summary and a "show diff" link.
* Long operations run on the existing worker thread; the UI never blocks on a
  dump.
* Every dialog is usable offline in dry-run mode.

### 3.4 Tests

* Unit tests for every frame builder and parser.
* Replay tests against `captures/fuse-scx2-2021-06-07.pcap` where applicable.
* CLI `--yes` tests use a fake transport, not a real amp.
* Hardware checklist per milestone in `docs/test-plan.md`.

## 4. Feature specs

### M1.1 Save the live buffer to an amp slot

**Goal:** The editor gets FUSE's SAVE button.

**Current:** `scx2 store --bank 1|2 --index N --name ...` works and is verified;
the editor has no equivalent.

**Protocol** (verified 2026-11-04):

```text
host -> 1c 01 03 <bank> <index> 00 01 01 <name at 16..48>
```

* `bank 1` = amp/voice presets (17 slots: 0 is "voice", 1..16 stored).
* `bank 2` = FX presets (16 slots).
* The amp writes the current live buffer and the 32-byte name.
* There is no per-block payload and no read-back in the frame.

**UI**

* "Save to Amp…" button next to Save As.
* Dialog: preset name (32 bytes, live preview of the truncated name), bank
  (Amp/FX), slot number, current slot name, overwrite warning.
* Defaults: current selected bank/slot if stored; otherwise the first empty
  slot.
* After save, select the written slot and refresh the dump.

**CLI:** keep `store` as the reference command.

**Safety:** read the target slot first; if non-empty, offer a backup export.

**Acceptance**

* Store to bank 1 and bank 2, read back, compare blocks and name.
* Name longer than 32 bytes is rejected/truncated with a warning, not silently.
* Overwrite warning names the slot being replaced.
* Power-cycle check documented as a manual test.

**Effort:** M.

### M1.2 Rename a stored preset

**Goal:** Rename without rewriting the sound.

**Current:** `scx2 rename` writes the `0x04` name block; the editor has no
action.

**Protocol:** `0x04` name block, 32-byte ASCII at offset 16.

**UI:** context menu / inline edit in the sidebar and in the save dialog.
Disallow empty names; trim trailing NULs; show the byte length.

**Acceptance:** rename a stored slot, read back the name, verify all other
bytes unchanged.

**Effort:** S.

### M1.3 Restore a backup to the amp

**Goal:** FUSE's Restore… action, from the editor.

**Current:** `scx2 restore <file>` works from the CLI; opening a backup in the
editor only loads it locally.

**Scope**

* whole dump,
* one bank (amp or FX),
* optionally one preset.

**Protocol:** write every block with `1c c3 <block>` followed by the `1c 03`
execute frame; the existing `restore` path already does this.

**UI**

* "Restore Backup…" file picker + scope selector.
* Before writing: summary of source device, preset count and target.
* Progress by block; cancel between blocks.
* After writing: automatic re-dump and diff.

**Safety:** require a fresh backup; show the target bank/slots; no automatic
factory reset.

**Acceptance:** restore a full dump and a single bank; re-dump matches except
the known execute flag; cancelled restore reports the partial state.

**Effort:** M.

### M5.1 Factory restore

**Goal:** Restore the amp's factory preset set without flashing firmware.

**Open question:** the factory data source. Candidates:

* the factory presets embedded in FUSE 2.7.1 / `Superchamp.upd`,
* a known-good factory dump captured from a fresh amp,
* a curated factory backup shipped with `scx2`.

Do not ship guessed data. The source must be reproducible and documented.

**UI:** "Factory Restore…" with a warning, backup requirement and a diff
against the current dump.

**Semantics:** write bank 1 and bank 2 (and the live buffer) from the factory
data. This is a preset restore, not a firmware reset and not a boot-mode
operation.

**Acceptance:** factory dump applied to a test amp; all 17 + 16 slots read back
as expected; live buffer restored; power-cycle check.

**Effort:** M (mostly data provenance and safety).

### M2.1 Preset library and browser

**Goal:** FUSE's PC/Amp preset organization.

**Data model**

* Local library folder of `.fuse` files and/or JSON presets.
* An index/cache with name, bank/slot (for amp presets), source device,
  modified time, tags/genres and a stable id.
* Unknown XML/JSON fields are preserved; metadata lives beside the preset, not
  inside device blocks.

**Operations**

* New Preset (clean live buffer with defaults).
* Open, Save As, Duplicate, Delete (local library only), Archive.
* PC vs Amp filter; previous/next; search by name; sort by name/date/slot.
* Drag a preset onto the editor to open; drag onto a slot to save.

**Device constraint:** amp presets cannot be deleted, only overwritten; deleting
a local library file does not touch the amp.

**Acceptance:** search/sort/filter over at least 100 presets; duplicate and save
round trip; no metadata loss; offline usable.

**Effort:** L.

### M2.2 New / duplicate / overwrite

* New Preset: choose a default voice (or "Clean Channel"), clear effects and
  set model defaults.
* Duplicate: copy the current live buffer or a library preset under a new name.
* Overwrite: the save-to-amp path with the overwrite warning.

**Acceptance:** New Preset is not a blank/invalid block; duplicate produces
independent files; overwrite only changes the target slot.

**Effort:** S–M.

### M3.1 Partial import from another preset

**Goal:** FUSE's IMPORT dialog for the SCX2.

**UI:** choose a source `.fuse`/backup preset; select modules:

* Amplifier (voice + advanced amp, cabinet),
* Stomp, Modulation, Delay, Reverb,
* optionally USB Gain.

Show source/destination and a compatibility warning when the source is another
product or an unknown model. Unknown models remain visible but are not applied
unless explicitly chosen.

**Protocol:** write only the selected blocks to the live buffer and send them;
do not touch unselected modules.

**Acceptance:** import only the reverb from a preset and verify amp/stomp/mod/
delay are byte-identical; unknown fields preserved in the file.

**Effort:** M.

### M3.2 SAVE EFFECTS / SAVE VOICE / SAVE CH1

**Goal:** Match FUSE's SCX2 sub-preset dialogs.

* **SAVE EFFECTS** writes the FX bank (bank 2): mod/delay/reverb, with a name.
* **SAVE VOICE** writes the amp/voice bank (bank 1): amp + stomp, with a name.
* **SAVE CH1** saves the clean-channel/voice settings (FUSE's `SaveMidKnobDialog`
  for the clean channel).

**Protocol:** the same store frame as M1.1. The amp stores the blocks that belong
to that bank; the UI should make clear which modules will be written.

**UI:** slot grids like FUSE's dialogs (Vib1/Vib2/Dly1… and 1..15), name field,
overwrite warning, read-back.

**Acceptance:** save effects to a bank 2 slot and verify bank 1 is unchanged;
save a voice to bank 1 and verify bank 2 is unchanged.

**Effort:** M.

### M4.1 USB Gain

**Goal:** FUSE's advanced USB GAIN control.

**Protocol**

* The live buffer's `0x0D` misc block carries USB Gain at offset 16.
* OpenFUSE implements it as a block with `0x0D`, byte 16 = gain, followed by
  the `1c 03 00` execute frame; `scx2` should reuse its verified block-write
  path and then read the misc block back.
* FUSE labels the range 0..255; document the mapping to 0..100% in the UI.

**UI:** a knob or numeric field in the advanced amp panel, next to the other
hidden controls. It is a device setting, not part of `.fuse`'s 23 amp params.

**Acceptance:** read current value, write a new value, read back, survive a
reconnect; no other block changes.

**Risk:** the exact write direction/execute path needs one hardware check.

**Effort:** S–M.

### M4.2 Preset metadata

**Goal:** FUSE's Preset Info dialog: description, tags, genres, author.

**Storage:** local library metadata (sidecar/index), not device bytes. The
32-byte device name remains the only name the amp stores.

**UI:** a Preset Info dialog for library presets; show read-only device
name/slot; editable description, tags, main genre and two sub-genres; author
field; optional rating/comments for compatibility with old FUSE exports.

**Export:** preserve the metadata when saving to the library; `.fuse` export
keeps the device name and, if we add an extension, does so without breaking
FUSE compatibility.

**Acceptance:** metadata survives library reload, duplicate and export; it does
not change any device block.

**Effort:** M.

### M5.2 Firmware update

**Goal:** Apply an official `Superchamp.upd` from `scx2`.

**Protocol / flow** (from `docs/firmware.md`):

* Update file: 272-byte header + raw image; MD5 covers bytes `16..`.
* Target IDs and product name are in the header.
* The amp must enter boot mode: power on while holding **Save**; boot HID PID
  `1ed8:0006`.
* Send the image after the header; report progress; reboot when done.

**UI/CLI:** `scx2 firmware update <file>` and an editor action. Parse and verify
the file before touching USB. Refuse a mismatched product or bad MD5.

**Safety:** requires `--yes`; refuses unless the amp is already in boot mode
(or offers instructions); never auto-enters boot mode; never flashes in tests;
document recovery (boot-mode reflash).

**Acceptance:** dry-run parses the official file; hardware test on a disposable
amp; after update, handshake reports the expected build; a failed transfer
leaves boot mode reachable.

**Effort:** L (hardware risk).

### M6.1 Amp-native tuner and mute

**Goal:** FUSE's Tuner window, driven by the amp.

**Current:** `scx2` uses the PC audio input and its own pitch detector; it cannot
mute the amp or read the amp's tuner state.

**Protocol:** unknown. FUSE has `SetTunerStateMessage`, `TunerDataMessage`,
`TunerMuteState`, `TunerUsesSnappingToTone`. Needs a FUSE USB capture while
opening/closing the tuner and while playing notes.

**UI:** same tuner view, with an amp source and a mute toggle; fall back to the
PC input when the amp is not connected.

**Acceptance:** note/cents update from the amp, mute toggles and is restored on
close, no stuck mute after disconnect.

**Effort:** M–L (research).

### M7.1 Local media library (optional)

**Goal:** FUSE's practice tool, not core librarian work.

* Open a local folder of WAV/MP3 files.
* Play/stop, loop, link a track to a preset.
* Tempo/transpose for MIDI files if we support them.
* Output device, sample rate and buffer selection.

**Acceptance:** a track plays while the editor is connected; preset links
survive reload; no device bytes change.

**Effort:** M.

## 5. Other-product FUSE features

These are in FUSE because it supports several amplifier families. They are
**not SCX2 features**. The roadmap keeps them visible so nobody mistakes them
for missing SCX2 work, and so a future multi-product version has a starting
point.

### 5.1 Expression pedal

**FUSE:** `ExpressionPedalDialog`, `DualExpressionPedalDialog`,
`FXAdjustDialog`, `VolumeMode`/`ExpressionMode`/`PedalMode`, heel/toe ranges,
`FirstExpressionPedal`/`SecondExpressionPedal` XML blocks.

**SCX2:** no expression pedal input; the physical UI hides it. OpenFUSE notes
that SCX2 preset files can still carry a `FirstExpressionPedal` block.

**Policy:** preserve the block on `.fuse` import/export, do not expose an editor
for it, do not send expression frames. If a future SCX2 firmware or a different
product is targeted, make it a separate milestone with captured frames.

### 5.2 FX loop bypass

**FUSE:** `FxLoopBypass`, `ToggleFxLoopBypass`, `FxLoopHandler`.

**SCX2:** not supported; OpenFUSE restricts the FX-loop state block to Mustang
Floor (`product 7`).

**Policy:** no UI, no frames. Preserve an imported `FxLoopBypass` value only if
the file format requires it; never write it to an SCX2.

### 5.3 SD card

**FUSE:** `Save Preset to SD Card`, `Save to Amp SD Card`, `Search the
amplifier's SD card`, `HasSDCard`.

**SCX2:** no SD card.

**Policy:** do not implement. If a future target has storage, add a storage
abstraction (local folder vs amp/SD) instead of branching in each feature.

### 5.4 Phrase sampler / looper

**FUSE:** `PhrasePanel`, `HasPhraseSampler`, `Record Phrase`, `Play Phrase`,
`Overdub Phrase`, `PhraseSamplerState`.

**SCX2:** no phrase sampler/looper.

**Policy:** not implemented. The PC-audio tuner is the only audio-side feature
we keep for the SCX2.

### 5.5 MIDI band tracks

**FUSE:** `AllowsMidiBandTracks`, `MidiParamsGrid`, MIDI visibility in the media
library.

**SCX2:** no MIDI track storage/playback; this belongs to the media-library
feature of other products.

**Policy:** if M7.1 is built, decide separately whether MIDI file playback is in
scope; it is not needed for preset editing.

### 5.6 Community

**FUSE:** `CommunityBrowser`, login, upload/download, ratings, comments.

**Status:** the FUSE service is gone. Local `.fuse` import/export is the
replacement. Do not build network code for a dead service.

## 6. Deliberately not planned

* Host-side preset recall. The SCX2 has no host recall command; the front-panel
  knob is the only way. A recall attempt can move the amp's current-slot pointer
  and is not safe.
* Recreating FUSE's Silverlight UI or its online account system.
* Emulating the amp DSP or adding amp models that the firmware does not contain.
* Writing to the amp's firmware except through the official update path in M5.2.

## 7. Open questions and research spikes

1. Where is the authoritative factory preset data for the SCX2?
2. Does the SCX2 accept the USB Gain block write exactly as OpenFUSE does, or
   does it need a different direction byte/execute?
3. Which frames drive the amp's built-in tuner and mute?
4. Can `.fuse` carry description/tags/genres without breaking FUSE, or should
   metadata live only in the local library?
5. Are preset colors/slot colors meaningful on the SCX2?
6. Is the "voice" slot (bank 1 index 0) writable with the normal store frame on
   all firmware builds?
7. Can a cancelled restore be made resumable, or is "stop after current block"
   the honest contract?

## 8. Suggested order

1. M1.1 save-to-amp, M1.2 rename, M1.3 restore — highest user value, lowest
   protocol risk.
2. M2 preset library and browser.
3. M3 partial import and sub-presets.
4. M4.1 USB Gain (small, needs one hardware check), then M4.2 metadata.
5. M5.1 factory restore (once the data source is settled).
6. M6 amp-native tuner.
7. M5.2 firmware update (last, highest risk).
8. M7 optional media and the other-product track only when the target changes.
