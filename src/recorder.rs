//! Record the amp's USB output (or any input) to a WAV take.
//!
//! The SCX2 exposes only a capture endpoint: its USB audio is a finished amp
//! sound, post-effects and post-cabinet. This recorder captures that stream
//! faithfully and writes a sidecar JSON next to the take with the live preset,
//! so a sound can be recalled later. It is not a dry DI — see the module note
//! in `ui::recorder`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde_json::{json, Value};

use crate::tuner::is_amp_input;

const EVENT_CAPACITY: usize = 64;
/// Four seconds of audio may wait for the writer. The audio callback never
/// blocks; if the queue fills, the oldest samples are dropped.
const QUEUE_SAMPLES: usize = 48_000 * 4;
const PROGRESS: Duration = Duration::from_millis(100);
const BITS_PER_SAMPLE: u16 = 24;
/// At or above this magnitude the sample will be hard-clipped by `to_i24`.
const CLIP_LEVEL: f32 = 0.999;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Starting,
    Recording,
    Error,
}

/// One capture endpoint, with the channel count its driver advertises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub channels: u16,
}

enum Event {
    Devices(Vec<Input>),
    Started {
        path: PathBuf,
    },
    Progress {
        seconds: f32,
        level: f32,
        clipped: bool,
    },
    Finished {
        path: PathBuf,
        sidecar: PathBuf,
        seconds: f32,
        frames: u64,
        peak: f32,
        clipped: bool,
    },
    Error(String),
}

/// A WAV recorder fed by one audio input.
pub struct Recorder {
    stop: Option<Sender<()>>,
    events: Option<Receiver<Event>>,
    worker: Option<JoinHandle<()>>,
    pub status: Status,
    pub devices: Vec<Input>,
    /// The input to record from; `None` means the amp, then the default.
    pub selected: Option<String>,
    /// Zero-based channel to take from the input. A guitar DI lives on one
    /// channel of an interface, not in a downmix of all of them.
    pub channel: usize,
    /// True once an input scan has been started.
    pub scanned: bool,
    pub path: Option<PathBuf>,
    pub sidecar: Option<PathBuf>,
    pub error: Option<String>,
    pub seconds: f32,
    /// Peak of the last progress window, linear 0..1.
    pub level: f32,
    /// Highest peak of the take so far, linear 0..1.
    pub peak: f32,
    /// Latched: the take reached full scale at least once.
    pub clipped: bool,
    pub frames: u64,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder {
            stop: None,
            events: None,
            worker: None,
            status: Status::Idle,
            devices: Vec::new(),
            selected: None,
            channel: 0,
            scanned: false,
            path: None,
            sidecar: None,
            error: None,
            seconds: 0.0,
            level: 0.0,
            peak: 0.0,
            clipped: false,
            frames: 0,
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record from `device`, or from the amp / default input when `None`.
    ///
    /// `meta` is written into the take's sidecar JSON. The recording thread
    /// owns the stream; the UI only sees progress events.
    pub fn start(&mut self, device: Option<String>, channel: usize, meta: Value) {
        self.stop();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = sync_channel(EVENT_CAPACITY);
        let error_tx = event_tx.clone();
        let worker = std::thread::Builder::new()
            .name("scx2-recorder".into())
            .spawn(move || {
                if let Err(error) = run(device, channel, meta, event_tx, stop_rx) {
                    let _ = error_tx.try_send(Event::Error(format!("{error:#}")));
                }
            });
        match worker {
            Ok(worker) => {
                self.stop = Some(stop_tx);
                self.events = Some(event_rx);
                self.worker = Some(worker);
                self.status = Status::Starting;
                self.error = None;
                self.seconds = 0.0;
                self.level = 0.0;
                self.peak = 0.0;
                self.clipped = false;
                self.frames = 0;
            }
            Err(error) => {
                self.status = Status::Error;
                self.error = Some(format!("Could not start the recorder: {error}"));
            }
        }
    }

    /// Stop recording and finalize the take.
    pub fn stop(&mut self) {
        if self.stop.is_none() && self.worker.is_none() {
            return;
        }
        self.stop = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.poll();
        if self.status != Status::Error {
            self.status = Status::Idle;
        }
        self.level = 0.0;
    }

    /// List the inputs in the background. Called when the panel opens and by
    /// the Rescan button.
    pub fn scan(&mut self) {
        if self.is_recording() {
            return;
        }
        self.scanned = true;
        let (event_tx, event_rx) = sync_channel(EVENT_CAPACITY);
        let worker = std::thread::Builder::new()
            .name("scx2-recorder-scan".into())
            .spawn(move || {
                let devices = input_devices()
                    .map(|inputs| inputs.into_iter().map(|(input, _)| input).collect())
                    .unwrap_or_default();
                let _ = event_tx.try_send(Event::Devices(devices));
            });
        if let Ok(worker) = worker {
            self.events = Some(event_rx);
            self.worker = Some(worker);
        }
    }

    pub fn poll(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.events {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                Event::Devices(devices) => self.devices = devices,
                Event::Started { path } => {
                    self.status = Status::Recording;
                    self.path = Some(path);
                    self.error = None;
                }
                Event::Progress {
                    seconds,
                    level,
                    clipped,
                } => {
                    self.seconds = seconds;
                    self.level = level;
                    self.peak = self.peak.max(level);
                    self.clipped |= clipped;
                }
                Event::Finished {
                    path,
                    sidecar,
                    seconds,
                    frames,
                    peak,
                    clipped,
                } => {
                    self.status = Status::Idle;
                    self.path = Some(path);
                    self.sidecar = Some(sidecar);
                    self.seconds = seconds;
                    self.frames = frames;
                    self.peak = self.peak.max(peak);
                    self.clipped |= clipped;
                    self.level = 0.0;
                    self.stop = None;
                }
                Event::Error(message) => {
                    self.status = Status::Error;
                    self.error = Some(message);
                    self.stop = None;
                    self.level = 0.0;
                }
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.status, Status::Starting | Status::Recording)
    }

    pub fn using_amp(&self) -> bool {
        self.selected.as_deref().is_some_and(is_amp_input)
    }

    /// Channel count of the selected input, or of the amp when on Auto.
    pub fn channels(&self) -> u16 {
        self.selected
            .as_deref()
            .and_then(|name| self.devices.iter().find(|input| input.name == name))
            .or_else(|| self.devices.iter().find(|input| is_amp_input(&input.name)))
            .map(|input| input.channels)
            .unwrap_or(1)
    }
}

/// The folder takes are written to: `Music/SCX2 Recordings` when the OS has a
/// home folder, otherwise a folder next to the working directory.
pub fn recordings_dir() -> PathBuf {
    for variable in ["USERPROFILE", "HOME"] {
        if let Some(home) = std::env::var_os(variable) {
            return PathBuf::from(home).join("Music").join("SCX2 Recordings");
        }
    }
    PathBuf::from("scx2-recordings")
}

/// Open the takes folder in the system file manager.
pub fn open_recordings() {
    let dir = recordings_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&dir).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
    }
}

fn input_devices() -> Result<Vec<(Input, cpal::Device)>> {
    let host = cpal::default_host();
    let mut inputs = Vec::new();
    for device in host.input_devices().context("listing audio inputs")? {
        let Ok(name) = device.name() else {
            continue;
        };
        let channels = device
            .supported_input_configs()
            .map(|configs| configs.map(|config| config.channels()).max().unwrap_or(1))
            .unwrap_or(1);
        inputs.push((Input { name, channels }, device));
    }
    Ok(inputs)
}

/// Pick a stream config that actually carries `channel`.
///
/// The device default is often a 2-channel mix, even on an interface that
/// advertises eight inputs. Opening that default would silently clamp a
/// higher channel, so ask for a range wide enough for the requested one.
fn capture_config(
    device: &cpal::Device,
    name: &str,
    channel: usize,
) -> Result<cpal::SupportedStreamConfig> {
    let default = device
        .default_input_config()
        .with_context(|| format!("opening audio input {name:?}"))?;
    if default.channels() as usize > channel {
        return Ok(default);
    }

    let needed = (channel + 1) as u16;
    let mut fallback = None;
    let ranges = device
        .supported_input_configs()
        .with_context(|| format!("listing input formats for {name:?}"))?;
    for range in ranges {
        if range.channels() < needed {
            continue;
        }
        // Stay at the device's default rate when the wider config allows it.
        let rate = default.sample_rate();
        let config = if rate >= range.min_sample_rate() && rate <= range.max_sample_rate() {
            range.with_sample_rate(rate)
        } else {
            range.with_max_sample_rate()
        };
        if config.sample_format() == default.sample_format() {
            return Ok(config);
        }
        fallback.get_or_insert(config);
    }

    fallback.ok_or_else(|| anyhow::anyhow!("{name:?} cannot open input channel {}", channel + 1))
}

fn run(
    wanted: Option<String>,
    channel: usize,
    meta: Value,
    events: SyncSender<Event>,
    stop: Receiver<()>,
) -> Result<()> {
    let host = cpal::default_host();
    let inputs = input_devices()?;
    if inputs.is_empty() {
        bail!("no audio input devices found");
    }
    let listed: Vec<Input> = inputs.iter().map(|(input, _)| input.clone()).collect();
    let _ = events.try_send(Event::Devices(listed));

    let index = choose_device(&inputs, wanted.as_deref(), host.default_input_device());
    let (input, device) = inputs
        .get(index)
        .map(|(input, device)| (input.clone(), device.clone()))
        .ok_or_else(|| anyhow::anyhow!("no usable audio input device"))?;
    let name = input.name;

    let supported = capture_config(&device, &name, channel)?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;
    let channel = channel.min(channels.saturating_sub(1));

    let dir = recordings_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating the takes folder {}", dir.display()))?;
    let path = unique_take_path(&dir);
    let sidecar = path.with_extension("json");
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: BITS_PER_SAMPLE,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(&path, spec).with_context(|| format!("creating {}", path.display()))?;
    write_sidecar(
        &sidecar,
        &path,
        &name,
        sample_rate,
        channel,
        channels,
        &meta,
        false,
        0,
        0.0,
        0.0,
        false,
    )?;
    let _ = events.try_send(Event::Started { path: path.clone() });

    let queue = Arc::new(Mutex::new(VecDeque::with_capacity(QUEUE_SAMPLES)));
    let stream = (match sample_format {
        cpal::SampleFormat::I8 => {
            build_stream::<i8>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::I32 => {
            build_stream::<i32>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::U8 => {
            build_stream::<u8>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::U32 => {
            build_stream::<u32>(&device, &config, channels, channel, &queue, &events)
        }
        cpal::SampleFormat::F64 => {
            build_stream::<f64>(&device, &config, channels, channel, &queue, &events)
        }
        other => bail!("unsupported audio sample format {other}"),
    })?;
    stream.play().context("starting the audio input")?;

    let mut frames: u64 = 0;
    let mut window_peak: f32 = 0.0;
    let mut window_clipped = false;
    let mut peak: f32 = 0.0;
    let mut clipped = false;
    let mut scratch: Vec<f32> = Vec::with_capacity(4096);
    let mut last_report = Instant::now();
    loop {
        scratch.clear();
        if let Ok(mut queue) = queue.lock() {
            scratch.extend(queue.drain(..));
        }
        for sample in &scratch {
            writer
                .write_sample(to_i24(*sample))
                .context("writing the take")?;
            frames += 1;
            let magnitude = sample.abs();
            window_peak = window_peak.max(magnitude);
            peak = peak.max(magnitude);
            if magnitude >= CLIP_LEVEL {
                window_clipped = true;
                clipped = true;
            }
        }
        if last_report.elapsed() >= PROGRESS {
            let seconds = frames as f32 / sample_rate as f32;
            let _ = events.try_send(Event::Progress {
                seconds,
                level: window_peak,
                clipped: window_clipped,
            });
            last_report = Instant::now();
            window_peak = 0.0;
            window_clipped = false;
        }
        let stopping = match stop.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => true,
            Err(TryRecvError::Empty) => false,
        };
        if stopping {
            break;
        }
        if scratch.is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // Let the stream go before draining, so the callback cannot add more.
    drop(stream);
    scratch.clear();
    if let Ok(mut queue) = queue.lock() {
        scratch.extend(queue.drain(..));
    }
    for sample in &scratch {
        writer
            .write_sample(to_i24(*sample))
            .context("writing the take")?;
        frames += 1;
        let magnitude = sample.abs();
        peak = peak.max(magnitude);
        clipped |= magnitude >= CLIP_LEVEL;
    }
    writer.finalize().context("finishing the take")?;

    let seconds = frames as f32 / sample_rate as f32;
    write_sidecar(
        &sidecar,
        &path,
        &name,
        sample_rate,
        channel,
        channels,
        &meta,
        true,
        frames,
        seconds,
        peak,
        clipped,
    )?;
    let _ = events.try_send(Event::Finished {
        path,
        sidecar,
        seconds,
        frames,
        peak,
        clipped,
    });
    Ok(())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    channel: usize,
    queue: &Arc<Mutex<VecDeque<f32>>>,
    events: &SyncSender<Event>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    let queue = Arc::clone(queue);
    let errors = events.clone();
    let channels = channels.max(1);
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // One lock per buffer, and never block the audio thread: a missed
            // buffer is better than a dropped-out take.
            let Ok(mut queue) = queue.try_lock() else {
                return;
            };
            for frame in data.chunks(channels) {
                let Some(sample) = frame.get(channel).or_else(|| frame.first()) else {
                    continue;
                };
                let sample = f32::from_sample(*sample);
                if !sample.is_finite() {
                    continue;
                }
                if queue.len() >= QUEUE_SAMPLES {
                    queue.pop_front();
                }
                queue.push_back(sample);
            }
        },
        move |error| {
            let _ = errors.try_send(Event::Error(format!("audio stream error: {error}")));
        },
        None,
    )?;
    Ok(stream)
}

/// Prefer the requested device, then the amp, then the system default.
fn choose_device(
    inputs: &[(Input, cpal::Device)],
    wanted: Option<&str>,
    default: Option<cpal::Device>,
) -> usize {
    if let Some(wanted) = wanted {
        if let Some(index) = inputs.iter().position(|(input, _)| input.name == wanted) {
            return index;
        }
    }
    if let Some(index) = inputs
        .iter()
        .position(|(input, _)| is_amp_input(&input.name))
    {
        return index;
    }
    if let Some(default) = default {
        if let Ok(name) = default.name() {
            if let Some(index) = inputs.iter().position(|(input, _)| input.name == name) {
                return index;
            }
        }
    }
    0
}

fn to_i24(sample: f32) -> i32 {
    (sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32
}

/// Linear peak to dBFS, with silence pinned at -120.
pub fn dbfs(peak: f32) -> f32 {
    if peak > 1e-6 {
        20.0 * peak.log10()
    } else {
        -120.0
    }
}

fn write_sidecar(
    sidecar: &Path,
    path: &Path,
    device: &str,
    sample_rate: u32,
    channel: usize,
    source_channels: usize,
    meta: &Value,
    complete: bool,
    frames: u64,
    seconds: f32,
    peak: f32,
    clipped: bool,
) -> Result<()> {
    let info = json!({
        "recorded_with": format!("scx2 {}", env!("CARGO_PKG_VERSION")),
        "recorded_at_unix": unix_now(),
        "source_device": device,
        "source_channel": channel + 1,
        "source_channels": source_channels,
        "file": path.file_name().and_then(|name| name.to_str()).unwrap_or("take.wav"),
        "sample_rate": sample_rate,
        "channels": 1,
        "bits_per_sample": BITS_PER_SAMPLE,
        "complete": complete,
        "frames": frames,
        "seconds": ((seconds * 1000.0).round() as f64) / 1000.0,
        "peak_dbfs": ((dbfs(peak) * 10.0).round() as f64) / 10.0,
        "clipped": clipped,
        "note": "Recorded from one input channel. The amp's USB output is post-effects and post-cabinet; for NAM/IR re-amping record the dry guitar from an interface input.",
        "session": meta,
    });
    let text = serde_json::to_string_pretty(&info)?;
    std::fs::write(sidecar, text)
        .with_context(|| format!("writing the take's metadata {}", sidecar.display()))
}

fn unique_take_path(dir: &Path) -> PathBuf {
    let base = format!("scx2-{}", stamp());
    let mut path = dir.join(format!("{base}.wav"));
    let mut suffix = 2;
    while path.exists() {
        path = dir.join(format!("{base}-{suffix}.wav"));
        suffix += 1;
    }
    path
}

fn stamp() -> String {
    let (year, month, day, hour, minute, second) = civil_from_unix(unix_now() as i64);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// UTC calendar fields for a Unix timestamp (Howard Hinnant's civil algorithm).
fn civil_from_unix(seconds: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (
        year as i32,
        month as u32,
        day as u32,
        hour as u32,
        minute as u32,
        second as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_dates() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil_from_unix(1_793_836_800), (2026, 11, 5, 0, 0, 0));
    }

    #[test]
    fn peak_converts_to_dbfs() {
        assert!(dbfs(1.0).abs() < 1e-6);
        assert!((dbfs(0.5) + 6.0206).abs() < 0.01);
        assert_eq!(dbfs(0.0), -120.0);
    }

    #[test]
    fn writes_a_24_bit_mono_take() {
        let dir = std::env::temp_dir().join(format!("scx2-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 24,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&path, spec).unwrap();
        for sample in [0.0f32, 0.5, -0.5, 1.1, -1.1] {
            writer.write_sample(to_i24(sample)).unwrap();
        }
        writer.finalize().unwrap();

        let mut reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 44_100);
        assert_eq!(reader.spec().bits_per_sample, 24);
        let samples: Vec<i32> = reader.samples::<i32>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 5);
        assert_eq!(samples[0], 0);
        assert!((samples[1] - 4_194_303).abs() <= 1);
        assert!((samples[2] + 4_194_303).abs() <= 1);
        assert_eq!(samples[3], 8_388_607);
        assert_eq!(samples[4], -8_388_607);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn take_names_do_not_collide() {
        let dir = std::env::temp_dir().join(format!("scx2-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_take_path(&dir);
        std::fs::write(&first, b"x").unwrap();
        let second = unique_take_path(&dir);
        assert_ne!(first, second);
        assert!(second.file_name().unwrap().to_string_lossy().contains("-2"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
