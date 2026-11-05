# Parameter map

Amp blocks (kind `0x05`) carry 32 parameter bytes at offsets 32..63. The amp
front panel only exposes five of them, FUSE adds Middle; the rest are reachable through the
Mustang-style direct-control frame (`scx2 param` / `scx2 probe`).

Everything below was measured on a Super Champ X2 (fw `May 30 2013`) with
`scx2 probe --from 0 --to 20 --value 64 --yes`, which writes each parameter
and reports the block offsets that changed.

The protocol always carries a raw byte (`0..255`), but the editor displays
every knob and parameter cell on a **0–10 scale** (`byte / 255 * 10`) so it
matches the amp's front panel. The byte sent to the amp is unchanged.

## Direct-control map

| Index | Offset(s) | Name | Front panel | Notes |
| --- | --- | --- | --- | --- |
| `0x00` | 32 | Volume | yes | channel volume |
| `0x01` | 33 | Gain | yes | input gain |
| `0x02` | 34 | Gain 2 | no | second gain stage |
| `0x03` | 35 | Master | no | output level |
| `0x04` | 36 | Treble | yes | tone stack |
| `0x05` | 37 | Middle | no, FUSE only | tone stack, set per voice |
| `0x06` | 38 | Bass | yes | tone stack |
| `0x07` | 39 | Presence | no | power-amp feedback |
| `0x08` | 40 | Resonance | no | FUSE names this Resonance; writes cleanly |
| `0x09` | 41 + 47 | Noise gate depth | no | FUSE's `NG Depth`; also moves the gate pair |
| `0x0a` | 42 | Bias | no | power-amp bias |
| `0x0f` | 41, 47, 48 | Noise gate | no | gate on/off plus threshold |
| `0x10` | 47, 48 | Noise gate threshold | no | 16-bit pair, big endian |
| `0x11` | 49 | Cabinet | no | FUSE's speaker simulation: 0 = off, 1..12 = cabinets |
| `0x13` | 51 | Sag | no | supply sag |
| `0x14` | 52 | Bright | no | inverted: low = bright on |

Indices `0x0b`..`0x0e` have no separate direct-control behaviour on the SCX2;
`0x0c`..`0x0e` are the voice bytes at offsets 44..46 and are written as a
block. FUSE's Super Champ X2 model table numbers the first eight as Volume,
Gain, Gain2, Master, Treble, Middle, Bass and Presence; its "Resonance",
"NG Depth" and "Cab" are the same parameters measured here as `0x08`, `0x09`
and `0x11`.

## Cabinet simulation (offset 49)

FUSE's amp editor exposes a **Cabinet** dropdown for every SCX2 voice. The
value is FUSE's cabinet id and lives at offset 49 (`ControlIndex="17"`).
`0` turns the speaker simulation off (direct); `1`..`12` select a cabinet.
The firmware stores a default cabinet for each voice.

| Id | Cabinet | Id | Cabinet |
| --- | --- | --- | --- |
| `0x00` | Off (direct) | `0x07` | 2x12 AC30 |
| `0x01` | 1x12 '57 Deluxe | `0x08` | 4x12 EVH |
| `0x02` | 4x10 '59 Bassman | `0x09` | 2x12 '65 Twin |
| `0x03` | 1x12 '65 Deluxe | `0x0a` | 4x12 Recto |
| `0x04` | 1x10 '65 Princeton | `0x0b` | 2x12 Super-Sonic |
| `0x05` | 1x8 '57 Champ | `0x0c` | 1x12 Super-Sonic |
| `0x06` | 4x12 JCM800 | | |

The ids and identities come from FUSE's own cabinet enum
(`eFXPARM_CABSIM_*` in `FUSEUI.dll`), not from a guess. The editor writes this
parameter through direct control index `0x11`, and `.fuse` export keeps it so
a saved preset does not lose its speaker simulation.

`44..46` holds the voice index (same value three times) and is written with a
normal block write, not direct control. `47`/`48` behave as one 16-bit gate
threshold and are written as a block too.

### Guessed hidden knobs

The editor shows the five front-panel knobs plus Middle by default and puts the rest
behind the `+` menu on the panel. Their hints are guesses based on the Mustang
parameter names in `docs/reference/amp.h`:

- **Gain 2** - second gain stage, used by the Supersonic-style voices.
- **Master** - output-stage level; the SCX2 has no master knob.
- **Presence** - tone control the SCX2 panel omits.
- **Resonance** - FUSE's name for the parameter at offset 40; likely a
  low-end/resonance trim.
- **Noise gate depth**, **Noise gate threshold** - the noise gate; FUSE calls
  them `NG Depth` and `NG Thresh`.
- **Bias**, **Sag**, **Bright** - power-amp behaviour.

The **Cabinet** parameter is not a knob: the combined amp/speaker picker (the
grille name plate or the Voice knob) chooses the voice and the cabinet together,
with a head-and-cab preview. See the section above.

The hints marked *Guessed* are still inferences about what the SCX2 does
(Gain 2, Master, Presence, Bias, Gate threshold, Sag and Bright). The rest,
including Resonance, Noise gate depth and Cabinet, use FUSE's own names.
Changing one is safe (the amp restores the value on preset recall), but the
name may still not match what you hear.

## Voice index (offsets 44..46)

| Value | Voice | `model` id |
| --- | --- | --- |
| `0x00` | Clean Channel | `0x00eb` |
| `0x01` | '57 Deluxe | `0x0067` |
| `0x02` | '59 Bassman | `0x0064` |
| `0x03` | '65 Deluxe | `0x0053` |
| `0x04` | '65 Princeton | `0x006a` |
| `0x05` | '65 Twin | `0x0075` |
| `0x06` | Super-Sonic | `0x0072` |
| `0x07` | British '60s | `0x0061` |
| `0x08` | Metal 2000 | `0x006d` |
| `0x09` | British '80s | `0x005e` |
| `0x0a` | American '90s | `0x005d` |
| `0x0b` | British '70s | `0x0079` |
| `0x0c` | '57 Champ | `0x007c` |
| `0x0d` | JazzMaster | `0x00ea` |

## Writing a parameter

The amp ignores block writes for offsets 32..42, 49, 51 and 52. Those must go
through the direct-control frame:

```text
05 c3 02 <model lo> <model hi> <index> <index> 0c 00 <magic lo> <magic hi>
```

`magic = round(byte * 257)`, so an offset byte round-trips exactly. `scx2
apply` and the editor both pick this path automatically; `scx2 param` sends
one frame directly.

After an execute frame the amp reports byte 7 as `0x01` on every block. That
is expected and excluded from `scx2 diff` output.

## Other blocks

Every block repeats the same layout: `slot` at 18, model id at 16, parameters
from 32. FUSE gives every effect model a six-slot parameter list, and the names
below are the ones FUSE itself uses. The names are **not yet verified on SCX2
hardware**; `scx2 learn` is still the way to confirm them. The bypass byte
(offset 22) *is* verified: `scx2 bypass` toggles it with FUSE's `19 c3` frame,
and a dump shows `0` for on and `1` for bypassed.

| Model | Block | Offsets 32..37 |
| --- | --- | --- |
| Compressor `0x0007` | stomp | Level, Thresh, Ratio, Attack, Release |
| Overdrive `0x003c` | stomp | Level, Gain, Low, Mid, High |
| Green Box `0x00ba` | stomp | Level, Gain, Tone, Blend |
| Sine Chorus `0x0012` | mod | Level, Rate, Depth, Avg Dly, LR Phase |
| Vibratone `0x002d` | mod | Level, Rotor, Depth, Fdbk, LR Phase |
| Sine Tremolo `0x0041` | mod | Level, Rate, Duty, LFO Clip, Tri Shape |
| Phaser `0x004f` | mod | Level, Rate, Depth, Fdbk, Shape |
| Mono Delay `0x0016` | delay | Level, Time, Fdbk, Bright, Atten |
| Tape Delay `0x002b` | delay | Level, Time, Fdbk, Flutter, Bright, Stereo |
| Spring '65 `0x000b` | reverb | Level, Decay, Dwell, Diff, Tone |
| Small Hall `0x0024` | reverb | Level, Decay, Dwell, Diff, Tone |

The full table lives in `src/models.rs` (`effect_params`), and it covers every
model in the Mustang-wide list, not only the ones this capture happened to use.

The editor makes these blocks editable: click an effect slot to open that
pedal's focused editor, choose a model (or clear it with `None`), and turn the
large labelled knobs for the effect's parameters. Each control shows its FUSE
name and its 0–10 value, and a drag or the wheel turns it; **Effect
Parameters** still opens all 32 bytes. Loading a model gives it that model's
default parameter bytes: FUSE's captured defaults where they are known,
otherwise a neutral `0x80` in every slot the model uses. A model that is not in
the default table therefore no longer inherits the previous effect's settings.

Loading a model into a preset that does not carry that block creates a frame
with the preset's bank/index and the observed default slot for the effect kind;
the live buffer always has all four blocks.

## How to read a block

```console
scx2 show backup.json --preset 0:0 --raw
```

prints every report of the live buffer with its offsets, which is the easiest
way to compare two states by eye.
