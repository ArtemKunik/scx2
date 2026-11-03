# Inside `Superchamp.upd`

What the update actually contains, and what that means for adding amp models.

The file is 229,708 bytes: a 272-byte header followed by the Super Champ X2
application image. The image is linked at `0x08003000` (a separate bootloader
lives below it and is not part of the update), so flash address
`0x08003000 + n` maps to image offset `n`.

## What is in the image

| Area | Contents |
| --- | --- |
| `0x00000..0x33a00` | ARM Thumb code, ~211 KB |
| `0x33be8..0x33c10` | build string `May 30 2013`, table of voice indices |
| `0x33c90..0x344d4` | factory preset names (Tweed Champ, '65 Twin, ...) |
| `0x34400..0x34620` | factory FX preset blocks (name + parameters) |
| `0x34650..0x34720` | USB descriptors: VID `1ED8`, PID `000E`, UTF-16 strings |
| `0x34728..0x34a00` | parameter names (`Gain2`, `Presence`, `NG Depth`, ...) |
| `0x360c4..0x361f8` | 13 voice records (see below) |
| `0x36c6b..0x37044` | model name strings, long and short forms |

The USB descriptors prove this is the USB/HID controller firmware:

```text
12 01 00 02 00 00 00 40 1e d8 00 0e 01 00 01 02 03 01
bLength 18, Device, USB 2.0, VID 1ED8, PID 000E, strings 1/2/3
...
UTF-16: "FMIC" ... "SuperChamp X2 Amp"
```

## The only amp-specific data: 13 records of 24 bytes

At image offset `0x360c4` there is one 24-byte record per voice:

```text
u16  magic (always 0x0100)
u16  model id
u8   defaults[12]     one byte for each parameter at offsets 32..43
u8   voice_index[3]   copied into offsets 44, 45, 46
u8   0
u8   0
u8   default cabinet
u8   voice_index
u8   1
```

Decoded (model ids and voice indices match the HID protocol exactly):

| # | model id | name | voice | defaults (32..43) | tail |
| --- | --- | --- | --- | --- | --- |
| 1 | `0x0064` | '59 Bassman | 2 | `aa a2 80 80 80 7a a2 91 80 80 80 80` | `02 02 02 00 00 02 02 01` |
| 2 | `0x0067` | '57 Deluxe | 1 | `aa 99 80 80 be 80 80 80 80 80 80 80` | `01 01 01 00 00 01 01 01` |
| 3 | `0x0053` | '65 Deluxe | 3 | `aa 71 00 ff 91 cf 38 00 00 00 80 00` | `03 03 03 00 00 03 03 01` |
| 4 | `0x006a` | '65 Princeton | 4 | `aa 55 00 ff 99 cc 4c 80 80 80 80 80` | `04 04 04 00 00 04 04 01` |
| 5 | `0x0061` | British '60s | 7 | `aa a2 80 63 99 80 b0 00 80 80 80 80` | `07 07 07 00 00 07 07 01` |
| 6 | `0x005e` | British '80s | 9 | `aa ff 80 7d aa 5b c4 80 80 80 80 80` | `09 09 09 01 00 06 09 01` |
| 7 | `0x005d` | American '90s | 10 | `aa 8e 80 66 a4 19 c7 71 80 80 80 80` | `0a 0a 0a 03 00 0a 0a 01` |
| 8 | `0x006d` | Metal 2000 | 8 | `aa a4 80 55 99 4c 91 8e 80 80 80 80` | `08 08 08 02 00 08 08 01` |
| 9 | `0x0075` | '65 Twin | 5 | `aa 55 80 63 b3 bb aa 80 80 80 80 80` | `05 05 05 00 00 09 05 01` |
| 10 | `0x0072` | Super-Sonic | 6 | `aa bb 82 55 99 a2 99 80 80 80 80 80` | `06 06 06 02 00 0c 06 01` |
| 11 | `0x0079` | British '70s | 11 | `aa ff 80 7d aa 5b c4 80 80 80 80 80` | `0b 0b 0b 01 00 08 0b 01` |
| 12 | `0x007c` | '57 Champ | 12 | `aa b3 00 ff 80 80 80 80 80 80 80 80` | `0c 0c 0c 00 00 05 0c 01` |
| 13 | `0x00ea` | JazzMaster | 13 | `80 80 80 be 80 80 80 80 80 80 80 80` | `0d 0d 0d 00 00 03 0d 01` |

Those 12 bytes are preset defaults (volume, gain, tone stack, ...), not signal
processing. Twenty-four bytes per voice is far too small to describe an amp
model, and no larger per-model block follows the table.

Note also that the Mustang-only ids (`0xf1` Studio Preamp, `0xf6` '57 Twin,
`0xf9` '60s Thrift, `0xfc` British Colour, `0xff` British Watts) are **not** in
this table. The SCX2 firmware was built for exactly these 13 voices.

## So can we add our own amps?

Not by editing this file. The package is the controller firmware plus factory
presets/metadata; the sound generation it selects by model id is not present
as editable data.

What is realistic:

1. **New presets / voicings** - yes. Any of the 13 voices with any parameter
   values, which is what scx2 already does.
2. **Unlocking a hidden Mustang voice** - worth one experiment. Send a
   Mustang-only model id (`0xf1`, `0xf6`, `0xf9`, `0xfc`, `0xff`) in a live
   block and see whether the amp accepts it or falls back. If the DSP shares
   programs with the Mustang, this is how you would find out. The firmware
   table suggests it will be rejected, but it is cheap to test and reversible
   (live buffer only, restored by recalling a preset).
3. **A genuinely new amp model** - needs the DSP layer, not this update:
   either replacing a fixed DSP program or writing new code for it, then
   patching this voice table and reflashing. The update header is only an MD5
   (`md5(bytes[16:]) == bytes[0..16]`), so the updater accepts modified images;
   the risk is the bootloader, which is not in the package, and a bad image
   can leave the amp needing boot-mode recovery.

## Why the model code is elsewhere

The update files for the whole family are all about the same size despite very
different model counts (application images, header excluded):

```text
Mustang I        208,564 bytes
SuperChamp X2    229,436 bytes
Mustang III      248,488 bytes
Mustang Floor    299,352 bytes
```

Mustang III has far more amp and effect models than the SCX2, yet its image is
only 8% larger. The difference is controller features, not model data, which
is consistent with the model library living outside the controller
application.
