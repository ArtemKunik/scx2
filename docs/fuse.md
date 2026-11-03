# Fender FUSE `.fuse` files

A `.fuse` file is a small XML document describing **one** preset. FUSE used it
for sharing and for the "open preset" action; it is not a full backup. `scx2`
can now read and write it.

## Shape

```xml
<?xml version="1.0" encoding="utf-8"?>
<Preset name="Tweed" amplifier="Mustang" productId="1">
  <Amplifier>
    <Module ID="3" POS="0" BypassState="0">
      <Param ControlIndex="0">43690</Param>
      <Param ControlIndex="1">38550</Param>
      ...
    </Module>
  </Amplifier>
  <FX>
    <Stompbox>
      <Module ID="19" POS="3" BypassState="0">
        <Param ControlIndex="0">32896</Param>
      </Module>
    </Stompbox>
    <Modulation />
    <Delay />
    <Reverb />
  </FX>
  <Info name="Tweed"></Info>
</Preset>
```

* `ID` is **FUSE's model id**, not the DSP model id. The mapping is in
  `src/models.rs` (`fuse_id` / `from_fuse_id`); FUSE used ids `0..11` for the
  standard amps, `19..25` for the first stompboxes, `26..36` for modulation,
  `37..41` for delay, `42..51` for reverb, and `100..113` for the V2 models.
* `POS` is the effect slot (the same byte the amp reports at offset 18).
* `BypassState` is `0` for on and `1` for off; it is stored at offset 22.
* Each `Param` is 16-bit and carries the same byte twice (`byte * 257`), which
  is exactly the value the amp's direct-control frame uses.
* The `Amplifier` module has no voice field; the amp model id selects the voice.

## Commands

```console
scx2 import song.fuse -o song.json
scx2 export backup.json --preset 1:7 -o song.fuse
scx2 show song.fuse --preset 0:0
```

`import` puts the preset in the live buffer of a one-preset JSON backup, which
is what FUSE does when you open a preset. `export` takes one preset out of any
backup (JSON or another `.fuse`) and writes the XML.

The desktop editor accepts `.fuse` in **Open…**, in **Save As…** and as a drag
target. Opening a `.fuse` replaces the editor's live buffer; saving as `.fuse`
writes the selected preset.

## Limits

* The file is a preset, not a bank: it has no bank/index and no per-slot
  coordinates. Import therefore uses the live buffer.
* Models that FUSE did not give an id (Mono Echo Filter, Stereo Tape Delay,
  Multitap Delay, Ping Pong Delay) are exported with the DSP id as a fallback;
  FUSE may not recognize those files.
* `.fuse` files do not carry the amp's raw reserved bytes. Import creates a
  clean block from the model and parameters; export writes the model,
  parameters, slot and bypass state.
* Writing a `.fuse` back to an amp still needs the store frame, which is not
  known yet. See `docs/protocol.md`.
