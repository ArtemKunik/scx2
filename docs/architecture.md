# How it works

The amp is a composite HID device (`1ED8:000E`) with a vendor-defined HID
interface carrying 64-byte reports.

- The frame layout, the handshake, the three banks and the model id tables are
  documented in [protocol.md](protocol.md).
- The known parameters are in [parameters.md](parameters.md).
- The `.fuse` XML format is in [fuse.md](fuse.md).
- The firmware that ships inside FUSE 2.7.1 is in
  [firmware.md](firmware.md) and [firmware-voices.md](firmware-voices.md).
- Which FUSE features are still missing is tracked in
  [fuse-coverage.md](fuse-coverage.md).
- The implementation plan is in [roadmap.md](roadmap.md).

## Source layout

```
src/main.rs        entry point
src/cli/           scan, dump, replay, apply, param/probe, restore
src/protocol/      frames, message decoding, dump/preset model, diff, JSON
src/params.rs      direct-control frames, magic table, knob catalogue
src/capture.rs     USBPcap (.pcap) reader for offline work
src/fuse.rs        Fender FUSE `.fuse` XML import/export
src/models.rs      DSP model id -> name tables, effect params/defaults, FUSE ids
src/transport.rs   hidapi device discovery and I/O
src/tuner.rs       USB audio capture and YIN pitch detection
src/recorder.rs    24-bit WAV takes with a preset sidecar
src/endpoint.rs    Windows capture-endpoint switch (absent elsewhere)
src/ui/            egui editor: app state, amp view, procedural materials
src/windows.rs     console detach and error dialog
```

## Tests

`cargo test` runs without hardware. The decoder is covered by tests against a
real FUSE capture, and the UI tests drive the app with the bundled demo capture.
The hardware checklist is in [test-plan.md](test-plan.md).
