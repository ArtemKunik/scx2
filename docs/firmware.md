# Firmware inside the original FUSE 2.7.1 package

Investigation of `FenderFUSE_FULL_2.7.1.exe` (the last official FUSE build) to
answer one question: **did the installer ship the amp firmware, or did FUSE
download it?**

Answer: it shipped it. The Super Champ X2 image is inside the installer, and it
is the same build this amp already reports.

## The installer

| | |
| --- | --- |
| File | `FenderFUSE_FULL_2.7.1.exe` |
| Size | 328,419,823 bytes |
| SHA-256 | `38bb32a2dff5549067efb99aa1d4caf70331a1b67b2882cf97fcea97c4d09f26` |
| Format | NSIS 2 (Deflate), readable with `7z l` / `7z x` |
| Payload dates | 2014-02-01 (`FUSEUI.xap`, `FirmwareUpdater.exe`) |

## Firmware that is part of the package

```text
Fender FUSE\DefaultFirmware\
    BroncoLow\Bronco_I.upd            2011-11-28
    GDEC3\gdec3.upd                   2010-09-08
    MustangFloor\MustangFloor.upd     2013-01-24
    MustangHigh\Mustang_III.upd       2012-07-13
    MustangLow\Mustang_I.upd          2012-07-13
    MustangMini\MustangMini.upd       2012-07-13
    MustangV2High\Mustang_V2III.upd   2013-05-14
    MustangV2Low\Mustang_IV2.upd      2013-05-14
    PassportMini\PassportMini.upd     2013-06-24
    SuperChampX2\Superchamp.upd       2013-05-31   <-- this amp
```

So the SCX2 firmware was not fetched from `fuse.fender.com`; it sat next to
`FirmwareUpdater.exe` and was available offline.

## `Superchamp.upd`

| | |
| --- | --- |
| Size | 229,708 bytes |
| MD5 | `e426f28a6d172902c403b4cab8e3308a` |
| SHA-256 | `f73ed85dad695cbf85844037b6f63a9157da624eb8eb17aac0fb1060de063d63` |

Layout:

```text
0x000  16  MD5 of everything from 0x010 to the end of the file
0x010   4  "FMIC"
0x014   3  01 02 03          target ids (SCX2 differs from every other .upd)
0x01a  10  "May 30 2013"     firmware build string
0x02d  10  "SuperChampX2"    product name
0x044   6  "fender"
0x05a      "Designed in Scottsdale Arizona USA. Copyright (c) May 30 2013 ..."
0x110      raw ARM Cortex-M image (229,436 bytes)
```

The self-check is real and can be reproduced:

```console
$ python -c "import hashlib;b=open('Superchamp.upd','rb').read();\
print(hashlib.md5(b[16:]).hexdigest()==b[:16].hex())"
True
```

The payload is a plain Cortex-M vector table, not encrypted and not compressed:

```text
0x110  SP        0x20000400
0x114  Reset     0x08003393
0x118  NMI       0x080033a1
0x11c  HardFault 0x080033a3
...
       SysTick   0x08003385
```

Addresses in `0x0800_xxxx` are STM32 internal flash, so the updater sends the
image as-is after the 272-byte metadata block. `DevComms.dll` carries a
`BootHeaderSize` setting that tells the updater how much of the file is header
rather than code.

## It is the build this amp already runs

Handshake from the connected amp:

```console
$ scx2 dump -o check.json
connected: SuperChamp X2 Amp [1ed8:000e]
firmware build: May 30 2013
```

`Superchamp.upd` says `May 30 2013` at offset `0x01a`. Same build, so flashing
the packaged file would be a no-op reinstall rather than an upgrade.

## How FUSE would flash it

`Fender FUSE\FirmwareUpdater.exe` (version `2.7.1.48`) is a small WinForms tool.
Its strings and type names show the whole flow:

- `SendUpdateFileOverHID`, `UpdateFirmware`, `FirmwareProgress`.
- `DEFAULT_FIRMWARE_BASE_PATH` / `DefaultFirmwarePath` - reads the packaged
  `DefaultFirmware\<device>\*.upd`.
- `BootPid_Superchamp` next to `BootPid_I_II`, `BootPid_III_IV_V`, `BootPid_Mini`,
  `BootPid_Floor` - each product has its own boot-mode USB PID.
- Messages: *"Your amplifier needs to be rebooted into 'Boot Mode'"*,
  *"Switch power on while holding 'Save' button to enter firmware update
  mode"*, *"Firmware file incorrect, this device is a {0} and the file targets
  {1}"*.
- `Invalid hash comparison on file` - the MD5 header above is what it checks.

The installer also carries a WinUSB driver for boot mode
(`DriversINF\MUSTANG_I_II_BOOT.inf`, `USB\VID_1ED8&PID_0006`) so the amp is
reachable over HID while its application firmware is not running.

## Practical result for scx2

* A firmware file is available offline at
  `%ProgramFiles(x86)%\Fender\FUSE\DefaultFirmware\SuperChampX2\Superchamp.upd`
  after installing FUSE 2.7.1.
* The file is `header + raw image`; the header is 272 bytes with an MD5 check.
* This amp already runs the packaged build, so there is nothing to gain from
  flashing it.
* Actually flashing would need the amp in boot mode (power on with **Save**
  held) and the boot-mode HID PID; `scx2` currently only speaks the normal
  `1ed8:000e` interface.
