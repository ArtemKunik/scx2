# The editor

The window mirrors the amplifier: Volume, Gain, Treble, Bass and Voice — the
five knobs on the real front panel — plus Middle from FUSE, then the effect
blocks.

Everything is drawn, not shipped as bitmaps: the cabinet follows the selected
voice's period (tweed, brownface, blackface, silverface, British, modern or
metal), and each effect model gets its own hardware face — a stompbox, wah
treadle, tape echo, spring reverb tank or rack processor — with knobs that sit
where the parameter bytes put them.

Every knob and parameter cell is shown on a **0–10 scale**, like the amp's front
panel; the full 0–255 byte is still what gets sent to the amp.

## Knobs and parameters

Dragging a knob changes the value; the `x` on the knob (or a right-click)
removes it again. The `+` on the panel lists the front-panel set and, below it,
the parameters only direct control can reach (presence, resonance, master, bias,
gate, sag, bright…). FUSE's own Super Champ X2 model table names most of them;
the few that are still guesses say so in their hint.

**Cabinet** is not a knob: it is chosen together with the voice in the amp &
speaker picker. The menu's **Device** section holds **USB Gain…**, the amp's USB
recording level (raw 0–255, shown with its percentage): a device setting,
written on its own and read back from the misc block. Every knob, hidden or not,
can be removed.

The knobs stay in sync with the amp. Turning a knob on the amp moves it on
screen, and dragging one on the Live Buffer changes the sound straight away, so
there is no write button. Stored presets are read-only in place: edit the live
buffer, then use **Save to Amp…** to write the slot it should live in. Without
an amp you can edit a backup and save it.

## Effect slots

The four effect slots are editable: click any slot to open a focused editor for
that pedal. The pedal is drawn large, and every parameter gets its own control
in a rack beside it, with the FUSE name above and the 0–10 value below. Drag a
control or scroll over it; hold Shift for fine control and double-click to reset
one control. The model list is one click away, and right-clicking a slot (or
hovering and pressing the `x`) clears it.

A loaded effect has an **ON/OFF** pill in its slot header: it bypasses the effect
without unloading it, so the model and every setting stay exactly where they
were. The knob names come from FUSE's own model table (Level, Time, Fdbk, Depth,
Mix and so on), and loading a model gives it that model's default parameter bytes
instead of inheriting the previous effect's settings.

Open **Effect Parameters** for the raw byte view of every effect block: all 32
parameters, with the FUSE name for the slots the model uses. Each loaded effect
also has a **Bypass** toggle (offset 22, FUSE's `BypassState`, verified on
hardware). The names and defaults come from FUSE's model table and are not yet
verified by ear, so the panel says so; `scx2 learn` is the way to confirm them.

## Amp & speaker picker

The **Voice** knob and the model plate on the grille open the **Amp & Speaker**
picker: a head-and-cab picture of the current rig, the fourteen SCX2 voices on
the left and FUSE's cabinet list on the right. Picking a voice or a speaker
updates the picture and the live buffer immediately, and the picker stays open so
both can be changed in one visit. Dragging the Voice knob still sweeps through
the voices.

## Toolbar

The toolbar has **Save to Amp…**, **Open…**, **Save As…**, **Save Backup**,
**Restore…**, **Undo**, **Redo** and **Revert**, with Ctrl+O / Ctrl+S / Ctrl+Z /
Ctrl+Shift+Z shortcuts.

**Save to Amp…** writes the live buffer into a bank/slot with FUSE's verified
save frame: it shows the name the amp will store, warns when it is about to
replace a named slot, offers to export that slot as a `.fuse` first, and reads
the slot back afterwards, comparing every payload byte and the name.

Right-click a stored preset in the sidebar to **Rename…** it: only the `0x04`
name block and an execute frame are written, and the read-back checks that no
other byte moved. The same menu has **Preset Info…** for description, tags,
genres, author, rating and comments; that lives in the JSON backup's `info`
object, never in a device block (and not in a `.fuse` export).

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
byte-identical, and an unknown model is shown and left unticked until you choose
it.

**Save effects…**, **Save voice…** and **Save CH1…** in the sidebar headers are
FUSE's sub-preset dialogs: bank 2 (mod/delay/reverb), bank 1 voices 0–15
(amp/cabinet/stomp), and the Clean Channel slot (amp 16, only when channel 1 is
selected). They share the Save-to-Amp dialog and read-back, which compares only
the blocks the target bank stores.

## Tuner

**Tuner** opens a polyphonic guitar tuner fed by the amp's USB audio output.
Click the button in the toolbar and strum: all six strings of standard tuning are
tracked at once, each with its own cents readout, and the note under the cursor
gets a large strobe display. A string turns green when it is within a few cents.

The tuner follows the amp's input automatically; **Rescan** re-lists devices, and
if Windows has the amp's capture endpoint disabled, **Enable amp input** switches
it on from inside the app (the same operation as the Sound control panel's Enable
item). On Linux the input appears on its own, so **Rescan** is enough.

The USB signal is post-amp and post-effects, so a clean voice with effects
bypassed gives the most reliable reading.

## Recorder

**Recorder** writes one input channel to a 24-bit WAV in `Music/SCX2 Recordings`,
with a `.json` sidecar holding the live preset (voice, cabinet, every block and
its raw bytes) so a take can be recalled. Pick the input and channel, then
**Record** / **Stop**. The meter latches the take's peak and flags clipping;
**Open folder** shows the takes. The full capture spec is in
[recording.md](recording.md).

For **Neural Amp Modeler / IR re-amping**, record the dry guitar: select the hi-Z
instrument input of an audio interface and the channel the guitar is on. The SCX2
cannot provide that signal — its USB is capture-only (it has no playback
endpoint), and the USB output is post-effects and post-cabinet, so **Auto (amp
USB)** records a finished amp sound, not a DI. A DI box with a through output, or
a splitter before the amp, lets you play through the amp while capturing the dry
signal. The sidecar records the source channel so a take's origin is unambiguous.

## MIDI foot controller

**MIDI** opens the foot-controller bridge. The amp has no MIDI input, so the
controller talks to `scx2`, which sends the same verified bypass frame as the
ON/OFF pill. Pick the MIDI input, press **Learn** on Stomp, Mod, Delay or Reverb,
and move the control you want to use; CC, note and program-change messages are
all accepted.

**Toggle** flips the effect on each press; **Follow** switches it on while the
control is on, which also suits a latching footswitch that sends 127/0. The
mappings are saved in `midi.json` under the platform config directory.

`scx2 midi --yes` runs the same bridge without the editor, which is the useful
mode for a laptop or small box left connected to the amp; `scx2 midi --list`
prints the available inputs.

MIDI on/off edits the live buffer. Recalling a preset on the amp's front panel
restores that preset's stored effect states, and preset recall itself still has
no host command. A computer must be running `scx2` between the controller and the
amp.
