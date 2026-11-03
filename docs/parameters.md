# Parameter map

Amp blocks (kind `0x05`) carry 32 parameter bytes at offsets 32..63. The
`scx2 learn` command fills in the missing rows below: it polls the live buffer
and prints which offsets move when you turn one knob.

## What is already known

### Offset 44..46, voice index

The same value appears at 44, 45 and 46 and it selects the amp voice. Taken
from the reference capture:

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

### Offsets 34..42, tone stack

These move between presets and are centred on `0x80`, the same convention the
Mustang family uses for bipolar parameters. `0x43` (offset 42) is `0x00` on
every "Clean" preset and `0x80` on the matching "Dirty" one, so it is the
first candidate for **gain**.

Reference rows from the capture:

```text
idx name                 32 33 34 35 36 37 38 39 40 41 42 43 | 44 45 46 47 48 49 50 51
 1  Tweed Deluxe Clean   00 00 80 80 00 80 00 80 80 80 00 00 | 01 01 01 00 00 01 01 01
 2  Tweed Deluxe Dirty   00 00 80 80 00 80 00 80 80 80 80 00 | 01 01 01 00 00 01 01 01
 6  '65 Deluxe           00 00 00 ff 00 cf 00 00 00 80 00 00 | 03 03 03 00 00 03 03 01
 7  '65 Twin             00 00 80 63 00 bb 00 80 80 80 80 00 | 05 05 05 00 00 09 05 01
16  Clean Channel        00 00 00 00 00 ff 00 00 00 00 00 00 | 00 00 00 00 00 00 00 01
```

Only the deltas between Clean and Dirty are certain so far. Everything else
needs a knob turn on real hardware.

### Offsets 49 and 50

Byte 50 tracks the voice index on most presets but not all (`'65 Twin` has 49
= `09`, 50 = `05`; `British '70s` has 49 = `06`, 50 = `0b`), which suggests 49
is a cabinet/speaker id and 50 the amp model. Unverified.

### Offset 51

Always `0x01` in the capture. Probably an "enabled" flag.

## Knob map (to fill during the test session)

Run `scx2 learn`, turn one knob, write down the offsets it reports.

| Control | Offset | Range seen | Notes |
| --- | --- | --- | --- |
| Volume | | | |
| Gain | | | |
| Treble | | | |
| Bass | | | |
| Voice | 44,45,46 | 0x00..0x0d | already known |
| FX level | | | |
| Reverb | | | |

## Other blocks

Every block repeats the same layout: `slot` at 18, model id at 16, parameters
from 32. The stomp/mod/delay/reverb parameter meanings are still unknown; the
same `learn` workflow maps them (turn FX level or reverb on the amp, or load a
different effect preset).

## How to read a block

```console
scx2 show backup.json --preset 0:0 --raw
```

prints every report of the live buffer with its offsets, which is the easiest
way to compare two states by eye.
