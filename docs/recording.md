# Recording takes and DI

`scx2` records one input channel to a 24-bit mono WAV. The main use is
capturing a dry guitar DI for Neural Amp Modeler and IR plugins, but the same
recorder captures the amp's finished USB output for ideas and reference takes.

The amp cannot supply a dry DI: its USB interface is capture-only and carries
the post-effects, post-cabinet output. For NAM/IR, record a hi-Z instrument
input on an audio interface (or a DI box) instead.

## Amp USB endpoint

Measured on the reference machine with cpal/WASAPI:

| Property | Value |
| --- | --- |
| Device | `Internal AUX Jack (SuperChamp X2 Amp)` |
| Direction | capture only |
| Channels | 2 (the amp is mono; both channels carry the same signal) |
| Sample rate | 44,100 Hz |
| Default sample format | `F32` |
| Supported sample formats | `U8`, `I16`, `I32`, `F32` |
| Signal | post-voice, post-effects, post-cabinet |
| Driver path | WASAPI shared mode |

A capture device that appears in Windows is not necessarily an enabled one.
If the amp's endpoint is disabled, **Enable amp input** in the tuner switches it
on through `IPolicyConfig::SetEndpointVisibility`; the recorder's **Rescan**
re-lists inputs afterwards.

## Recorder output

| Property | Value |
| --- | --- |
| Container | RIFF/WAVE (`.wav`) |
| Encoding | PCM, 24-bit little-endian |
| Channels written | 1 (mono) |
| Source channel | selectable: Ch 1 … Ch *n* |
| Sample rate | the selected device's default rate |
| Gain / normalize / dither / EQ | none |
| Processing | channel select → `f32` → clamp → 24-bit |
| Monitoring | none |
| Simultaneous takes | one |
| Finalization | on **Stop** or on hiding the panel |

The 24-bit file is the storage format. Effective resolution is whatever the
device delivers; the app does not add gain or dither.

### Size

| Rate | Bytes/min | MiB/min | MiB/hour |
| --- | --- | --- | --- |
| 44,100 Hz | 7,938,000 | 7.57 | 454 |
| 48,000 Hz | 8,640,000 | 8.24 | 494 |

### Levels and clipping

- The meter shows the current 100 ms window and a **latched peak** for the take.
- A sample at or above `0.999` full scale latches **CLIP**; `to_i24` clamps it,
  so the file cannot recover it.
- The sidecar records `peak_dbfs` and `clipped` for the whole take.
- There is no input gain in the app. Set the interface's gain so the loudest
  playing peaks below full scale; for NAM captures, leave a few dB of headroom
  and use the plugin's input gain rather than a clipped take.
- The clip latch resets when the next recording starts.

### Multichannel inputs

The picker lists the maximum channel count advertised by the driver. The
stream does not assume that default mix: `capture_config` opens a supported
config with at least the selected number of channels, preferring the device's
default sample rate and format. If no such config exists, recording fails with
an explicit error instead of silently taking another channel. The sidecar's
`source_channel` and `source_channels` describe the stream that was actually
opened.

## Files and naming

```
%USERPROFILE%\Music\SCX2 Recordings\
  scx2-20261105-121505.wav
  scx2-20261105-121505.json
```

Names are UTC timestamps (`YYYYMMDD-HHMMSS`); collisions get `-2`, `-3`, ….
The folder falls back to `$HOME/Music/SCX2 Recordings`, then
`./scx2-recordings` when no home directory is available. **Open folder** opens
it in the system file manager.

## Sidecar

Written at start with `complete: false` and rewritten at Stop with the final
length, peak and clip state:

```json
{
  "recorded_with": "scx2 0.2.0",
  "recorded_at_unix": 1793880907,
  "source_device": "Internal AUX Jack (SuperChamp X2 Amp)",
  "source_channel": 1,
  "source_channels": 2,
  "file": "scx2-20261105-121505.wav",
  "sample_rate": 44100,
  "channels": 1,
  "bits_per_sample": 24,
  "complete": true,
  "frames": 83349,
  "seconds": 1.89,
  "peak_dbfs": -18.4,
  "clipped": false,
  "note": "Recorded from one input channel. ...",
  "session": {
    "app": "scx2 0.2.0",
    "selected": { "bank": 0, "index": 0 },
    "live": { "...": "full preset JSON, raw bytes included" },
    "amp": { "voice": 0, "cabinet": 1, "model": 0 }
  }
}
```

The session snapshot is taken when recording starts. It is context for an amp
take; a DI does not depend on it.

## DI → NAM/IR workflow

```
guitar ──► interface hi-Z input ──► scx2 Recorder ──► DI WAV
                                                        │
                                                        ▼
                                                DAW: NAM → IR
```

1. Select the interface under **Recorder**, then the channel the guitar is on.
2. Play the loudest part; keep the peak below full scale. The latch shows
   **CLIP** if the take is unusable.
3. Record, then open the WAV in the DAW and process it with NAM and an IR.

To play through the amp at the same time, split the guitar before the amp:

```
guitar ──► DI box / splitter ──► amp input
                    └──────────► interface input (dry)
```

## Re-amping through the real amp

The amp's USB cannot receive audio, so a hardware re-amp needs:

1. the DI WAV,
2. an interface output,
3. a **reamp box** (line level/low impedance → instrument level/high
   impedance),
4. the amp's instrument input.

`scx2` does not play WAVs to an output yet; the DAW can drive that chain. For
NAM/IR re-amping the DAW is the whole path and the amp is not involved.

## Known limitations

- **No dry output on the SCX2.** USB and the amp's line/headphone output are
  post-amp.
- **No monitoring.** Use the interface's direct monitor or the DAW.
- **One channel per take.** DI and amp/mic cannot be captured as one grouped
  take.
- **No ASIO / exclusive mode / buffer selection.** WASAPI shared, device
  default rate.
- **No crash recovery.** A hard kill mid-take leaves an unfinalized WAV.
- **No playback.** See above.
- **No DI/wet role field.** The sidecar identifies the source device and
  channel, but the app does not label a take as DI or amp.

## Implementation

| Path | Role |
| --- | --- |
| `src/recorder.rs` | capture stream, channel config, WAV writer, sidecar |
| `src/ui/recorder.rs` | panel, source/channel picker, meter, clip latch |
| `src/tuner.rs` | `is_amp_input`, shared amp-name matching |
| `src/endpoint.rs` | enabling a disabled Windows capture endpoint |
