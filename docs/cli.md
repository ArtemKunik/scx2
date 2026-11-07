# Command line reference

The `scx2` binary opens the desktop editor when started with no arguments.
Every command below is the same binary from a terminal.

## Reading

```console
scx2 scan                       # list Fender HID devices
scx2 scan --all                 # every HID device, not just Fender
scx2 dump -o backup.json        # read all presets into a JSON backup
scx2 show backup.json           # print a backup
scx2 show backup.json --preset 1:7 --raw
scx2 diff a.json b.json         # byte-level comparison of two backups
scx2 learn                      # watch the live buffer, map knobs to offsets
scx2 monitor                    # print every report the amp sends
scx2 replay capture.pcap        # decode a USBPcap recording, no amp needed
scx2 import song.fuse -o song.json   # import a Fender FUSE preset
scx2 export backup.json --preset 1:7 -o song.fuse
scx2 recall 7 --bank 1          # read-only: prints the slot
```

## Writing

Writes are dry runs unless `--yes` is given.

```console
scx2 apply amp --offset 35 --value 0x63 --from backup.json
scx2 send --hex "1c c1 05 00 00 00 01 01"
scx2 restore backup.json
scx2 store --bank 1 --index 7 --name "My preset"   # verified on hardware
scx2 rename "My preset" --bank 1 --index 7         # verified on hardware
scx2 bypass reverb 5 --off                         # verified on hardware
```

## MIDI

```console
scx2 midi --list                # list MIDI inputs
scx2 midi --yes                 # headless foot-controller bridge
```

## Output

`dump` and `replay` both print the full preset memory:

```text
34 preset(s)
bank  idx  name                       amp               stomp             mod             delay         reverb
--------------------------------------------------------------------------------------------------------------
live    0                             Clean Channel     None              None            Tape Delay    Small Hall
amp     0  voice                      '57 Champ         None              -               -             -
amp     1  Tweed Deluxe Clean         '57 Deluxe        None              -               -             -
amp     2  Tweed Deluxe Dirty         '57 Deluxe        Compressor        -               -             -
...
fx      6  Fender Spring Reverb       -                 -                 None            None          Spring '65
fx      11  Chorus + Reverb            -                 -                 Sine Chorus     None          Small Hall
```

Backups are lossless: every block keeps its raw 64-byte report, so a restore is
byte-for-byte and future versions can decode more from the same file.

## Safety

Every write command prints the exact frame it would send and stops unless
`--yes` is given. `restore` additionally requires a full backup first (make one
with `dump`). `store` writes the live buffer into a stored slot, so keep a
backup; `rename` writes only the `0x04` name block. Recall is read-only because
the SCX2 has no host preset-load command.
