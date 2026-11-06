# FUSE feature coverage

What the original Fender FUSE 2.7.1 offered, and what `scx2` has today.

The implementation plan for the gaps is in [roadmap.md](roadmap.md).

The FUSE side is taken from the last official build (`FUSEUI.dll` in
`FenderFUSE_FULL_2.7.1.exe`): the main menu, `PresetOrganizationPanel`,
`PresetEditor`, `PresetInfoDialog`, `BackupDialog`, `RestoreDialog`,
`FirmwareUpdateDialog`, `TunerWindow`, `AdvancedAmpSettingsPanel`,
`CommunityBrowser`, `MediaLibrary`, `BandTrackDialog` and `PhrasePanel`.

Status: **yes** = feature present, **partial** = works but incomplete or CLI
only, **missing** = not in `scx2`, **blocked** = the SCX2/protocol cannot do it,
**n/a** = FUSE had it for another product but the SCX2 has no such hardware.

## Editing

| FUSE feature | Status | Notes |
| --- | --- | --- |
| Amp voice/model selection | yes | Fourteen SCX2 voices, model id follows the voice. |
| Cabinet simulation | yes | FUSE's `Cab` at amp offset 49; choose it with the voice in the amp/speaker picker. |
| Advanced amp controls | partial | Gain 2, Master, Presence, Resonance, gate, bias, sag and bright are there. **USB Gain** is not. |
| USB Gain | missing | FUSE's SCX2 advanced panel has a USB GAIN knob. It lives in the `0x0D` misc block at offset 16. |
| Effect model selection | yes | Stomp, modulation, delay and reverb slots; click a slot for its focused editor. |
| Effect parameters | yes | Big labelled controls per parameter, plus all 32 raw bytes with FUSE names. |
| Effect bypass | yes | Verified `19 c3` frame; the model and settings stay loaded. |
| Effect defaults / reset | yes | Loading a model writes its defaults; reset restores them. |
| Signal-chain reordering | n/a | The SCX2 has four fixed, typed effect slots. FUSE's orderable-chain UI is for other products. |
| Undo / redo | yes | Whole-dump snapshots. |
| Open/save `.fuse` | yes | Whole-preset XML import/export. |
| Partial import from another preset | yes | **Import…** in the sidebar takes a `.fuse`/backup preset and copies only the ticked amp/stomp/mod/delay/reverb blocks into the live buffer; unselected blocks stay untouched. Unknown models are shown and left unticked. |
| Preset metadata | missing | FUSE has description, tags, genres, author, rating and comments. `scx2` only keeps the 32-byte name. |
| Amp theme / visual skin | missing | `scx2` picks a period look from the voice; FUSE had an `Amp Theme` menu item. |

## Presets and librarian

| FUSE feature | Status | Notes |
| --- | --- | --- |
| Live buffer editing | yes | Every edit is sent immediately while connected. |
| New preset | missing | No "start from empty" action in the editor. |
| Open a preset file | yes | JSON backup or `.fuse`. |
| Save a preset file | yes | Save As writes JSON or `.fuse`. |
| Save the live buffer to an amp slot | yes | `scx2 store` is the reference and is verified; the editor's **Save to Amp…** sends the same frame, warns before replacing a named slot, offers a `.fuse` backup and checks the read-back. |
| Rename a stored preset | yes | `scx2 rename` is the reference; right-click a stored preset in the editor and choose **Rename…**. Only the name block is written, and the other bytes are compared afterwards. |
| Save effects / voice sub-presets | partial | Storing to bank 2 / bank 1 works from the CLI. FUSE's SAVE EFFECTS, SAVE CH2 VOICE and SAVE CH1 dialogs are not in the editor. |
| Delete / overwrite a preset | partial | Storing overwrites a slot. FUSE itself says amp presets cannot be deleted, only overwritten. |
| Preset browser | missing | FUSE filters PC vs. amp presets, browses previous/next, searches and sorts. `scx2` lists the current dump only. |
| Preset info dialog | missing | No description/tags/genres/author editing. |
| Backup | partial | `dump` / Save Backup writes a whole-dump JSON. No description, backup list or single-preset selection in the UI. |
| Restore | yes | `scx2 restore` is the reference; **Restore…** in the editor takes a backup first, writes block by block with progress and cancel, and compares the re-dump with the backup. The backup's live buffer is not written. |
| Factory restore | missing | FUSE can reset the amp's presets/device state to factory defaults. |
| Recall a stored preset from the host | blocked | The SCX2 has no host recall command; the front-panel knob is the only way. |

## Device and audio

| FUSE feature | Status | Notes |
| --- | --- | --- |
| Scan / connect / dump | yes | Handshake, three banks, live polling. |
| Store and rename | yes | Both have editor actions: **Save to Amp…** and the sidebar's **Rename…**. |
| Effect bypass | yes | Verified fast frame. |
| Firmware update | missing | `docs/firmware.md` documents the update file and boot mode, but `scx2` cannot enter boot mode or flash. |
| Factory restore | missing | Not implemented. |
| Audio Control Panel | partial | No USB gain control. FUSE's SCX2 advanced panel exposes USB GAIN; the rest of the menu is mostly OS audio setup. |
| Amp-native tuner / mute | missing | FUSE drives the amp's own tuner and mute. `scx2` detects pitch from the USB audio input instead. |
| Firmware version display | yes | The handshake banner shows the build string. |
| About / check for updates | missing | Minor; FUSE had both. |

## Community and media

| FUSE feature | Status | Notes |
| --- | --- | --- |
| Login / Web Community | missing | The FUSE service is gone; no point rebuilding it. |
| Upload / download / search community presets | missing | Import/export `.fuse` is the local replacement. |
| Community ratings and comments | missing | The service is gone. |
| Media Library / local band tracks | missing | FUSE played local backing tracks and linked them to presets. A PC player is possible; the SD-card/amp-storage variants are n/a. |
| Phrase sampler / looper | n/a | No phrase sampler on the SCX2. |
| MIDI backing tracks | missing | Part of FUSE's media library; same local-file caveat. |
| Expression pedal assignments | n/a | No expression pedal input on the SCX2; FUSE's `.fuse` format can carry the block but the hardware UI hides it. |
| FX loop bypass | n/a | Mustang Floor only. |
| SD card / save to SD | n/a | No SD card. |

## What to do next

The largest remaining gaps are librarian/device operations, not sound editing:

1. **Save-to-amp, rename and restore in the editor.** Done: the verified
   frames are wired into the toolbar and sidebar, each with a read-back check.
2. **Preset browser and new/duplicate/overwrite.** Add PC-preset browsing,
   search/filter, previous/next and a "New Preset" action.
3. **Partial import and sub-presets.** Partial import is done; SAVE EFFECTS /
   SAVE VOICE still need the slot-grid dialogs.
4. **USB Gain.** Read and write the `0x0D` misc block at offset 16 and add a
   knob to the advanced amp panel.
5. **Preset metadata / local library.** Description, tags, genres, author and a
   searchable folder of presets.
6. **Factory restore and firmware update.** Both need boot-mode work and are
   the riskiest items; factory restore should come after a verified recovery
   path.
7. **Amp-native tuner/mute.** Requires mapping the tuner frames; the current
   PC-audio tuner is a workable fallback.
8. **Community and media.** Skip the community service (it is dead). A local
   backing-track/media library is optional practice tooling, not core librarian
   work; the SCX2 has no SD card or looper.
