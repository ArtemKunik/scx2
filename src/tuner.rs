//! USB audio capture and pitch detection for the tuner.
//!
//! The Super Champ X2 exposes its amp output as a USB capture device. We open
//! that input with cpal, downmix it to mono and run YIN on a short window. The
//! audio stream lives on its own thread; the UI only sees note readings.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};

/// Guitar range, with a little room for drop tunings.
const MIN_FREQ: f32 = 55.0;
const MAX_FREQ: f32 = 1200.0;
/// About 120 ms of audio. Low E gets several cycles, which YIN needs.
const WINDOW_SECONDS: f32 = 0.12;
/// Below this RMS the input is treated as silence, not a note.
const SILENCE: f32 = 0.0008;
const YIN_THRESHOLD: f32 = 0.15;
const EVENT_CAPACITY: usize = 128;
/// Two seconds of audio may wait for the analysis thread. It never blocks the
/// audio callback; if it does run full, the oldest samples are dropped.
const QUEUE_SAMPLES: usize = 48_000 * 2;

/// Names that identify the amp's USB capture endpoint, lower-case.
///
/// Windows names the endpoint after the jack (`Internal AUX Jack (SuperChamp X2
/// Amp)`). ALSA names it after the card, and the card id is the USB product
/// string cut down to `Amp`, so its PCMs read `hw:CARD=Amp,DEV=0`.
const AMP_INPUT_HINTS: [&str; 4] = [
    "superchamp",
    "super champ",
    "internal aux jack",
    "card=amp",
];

/// Whether an audio input name looks like the amp's USB capture endpoint.
pub fn is_amp_input(name: &str) -> bool {
    let name = name.to_lowercase();
    AMP_INPUT_HINTS.iter().any(|hint| name.contains(hint))
}

/// One pitch estimate and the signal level that produced it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub frequency: Option<f32>,
    pub level: f32,
}

impl Default for Reading {
    fn default() -> Self {
        Reading {
            frequency: None,
            level: 0.0,
        }
    }
}

/// Standard tuning, low string first: (name, frequency at A4 = 440 Hz).
pub const STRINGS: [(&str, f32); 6] = [
    ("E2", 82.4069),
    ("A2", 110.0),
    ("D3", 146.8324),
    ("G3", 195.9977),
    ("B3", 246.9417),
    ("E4", 329.6276),
];

/// How one open string is doing, whether or not the others are ringing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StringReading {
    /// False when the string is not part of what is sounding.
    pub active: bool,
    /// Negative is flat, positive sharp. Only meaningful when `active`.
    pub cents: f32,
    /// Strength of the string's fundamental and second harmonic.
    pub level: f32,
}

/// Everything one analysis window produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Analysis {
    /// The monophonic estimate, used for the large note readout.
    pub reading: Reading,
    /// Per string estimates, used for the polyphonic row.
    pub strings: [StringReading; 6],
}

impl Default for Analysis {
    fn default() -> Self {
        Analysis {
            reading: Reading::default(),
            strings: [StringReading::default(); 6],
        }
    }
}

/// A frequency rounded to the nearest equal-tempered note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub name: &'static str,
    pub octave: i32,
    /// Negative means flat, positive means sharp.
    pub cents: f32,
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Nearest note at A4 = 440 Hz.
pub fn note_for(frequency: f32) -> Option<Note> {
    if !frequency.is_finite() || frequency <= 0.0 {
        return None;
    }
    let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
    let nearest = midi.round();
    let cents = (midi - nearest) * 100.0;
    let index = nearest.rem_euclid(12.0) as usize;
    let octave = (nearest / 12.0).floor() as i32 - 1;
    Some(Note {
        name: NOTE_NAMES[index],
        octave,
        cents: cents.clamp(-50.0, 50.0),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Starting,
    Running,
    Error,
}

enum Event {
    Devices(Vec<String>),
    Running(String),
    Reading(Analysis),
    Error(String),
}

/// The UI-facing half of the tuner. The audio stream and DSP run on a worker
/// thread so an unavailable device or a slow callback cannot stall egui.
pub struct Tuner {
    stop: Option<Sender<()>>,
    events: Option<Receiver<Event>>,
    worker: Option<JoinHandle<()>>,
    pub devices: Vec<String>,
    pub selected: Option<String>,
    pub status: Status,
    pub reading: Reading,
    pub strings: [StringReading; 6],
    pub error: Option<String>,
}

impl Default for Tuner {
    fn default() -> Self {
        Tuner {
            stop: None,
            events: None,
            worker: None,
            devices: Vec::new(),
            selected: None,
            status: Status::Idle,
            reading: Reading::default(),
            strings: [StringReading::default(); 6],
            error: None,
        }
    }
}

impl Drop for Tuner {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Tuner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start listening on `device`, or on the amp / default input when `None`.
    ///
    /// Replacing the stream is synchronous: the old worker gets a moment to
    /// release the device before the new one opens it.
    pub fn start(&mut self, device: Option<String>) {
        self.stop();
        let (stop_tx, stop_rx) = channel();
        let (event_tx, event_rx) = sync_channel(EVENT_CAPACITY);
        let wanted = device.clone();
        let error_tx = event_tx.clone();
        let worker = std::thread::Builder::new()
            .name("scx2-tuner".into())
            .spawn(move || {
                if let Err(error) = run_audio(wanted, event_tx, stop_rx) {
                    let _ = error_tx.try_send(Event::Error(format!("{error:#}")));
                }
            });
        match worker {
            Ok(worker) => {
                self.stop = Some(stop_tx);
                self.events = Some(event_rx);
                self.worker = Some(worker);
                self.status = Status::Starting;
                self.selected = device;
                self.error = None;
            }
            Err(error) => {
                self.status = Status::Error;
                self.error = Some(format!("Could not start the audio thread: {error}"));
            }
        }
    }

    /// Stop listening and release the input device.
    pub fn stop(&mut self) {
        // Drop the stop sender first: the worker notices the disconnect even
        // if it is between analysis windows. Dropping the receiver also makes
        // any pending try_send fail immediately instead of blocking.
        self.stop = None;
        self.events = None;
        if let Some(worker) = self.worker.take() {
            crate::audio::join_bounded(worker, crate::audio::RELEASE_TIMEOUT);
        }
        self.status = Status::Idle;
        self.reading = Reading::default();
        self.strings = [StringReading::default(); 6];
    }

    /// Collect what the audio thread has produced since the last frame.
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
                Event::Running(name) => {
                    self.status = Status::Running;
                    self.selected = Some(name);
                    self.error = None;
                }
                Event::Reading(analysis) => {
                    self.reading = analysis.reading;
                    self.strings = analysis.strings;
                }
                Event::Error(message) => {
                    self.status = Status::Error;
                    self.error = Some(message);
                    self.stop = None;
                    self.events = None;
                    self.reading = Reading::default();
                    self.strings = [StringReading::default(); 6];
                }
            }
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, Status::Starting | Status::Running)
    }

    /// Whether the stream that is running is the amp's USB capture endpoint.
    pub fn using_amp(&self) -> bool {
        self.selected.as_deref().is_some_and(is_amp_input)
    }

    pub fn frequency(&self) -> Option<f32> {
        self.reading.frequency
    }

    pub fn note(&self) -> Option<Note> {
        self.reading.frequency.and_then(note_for)
    }
}

fn run_audio(wanted: Option<String>, events: SyncSender<Event>, stop: Receiver<()>) -> Result<()> {
    let host = cpal::default_host();
    let mut inputs = Vec::new();
    for device in host.input_devices().context("listing audio inputs")? {
        if let Ok(name) = device.name() {
            inputs.push((name, device));
        }
    }
    if inputs.is_empty() {
        bail!("no audio input devices found");
    }
    let names: Vec<String> = inputs.iter().map(|(name, _)| name.clone()).collect();
    let _ = events.try_send(Event::Devices(names));

    let index = choose_device(&inputs, wanted.as_deref(), host.default_input_device());
    let (name, device) = inputs
        .get(index)
        .map(|(name, device)| (name.clone(), device.clone()))
        .ok_or_else(|| anyhow!("no usable audio input device"))?;

    let supported = device
        .default_input_config()
        .with_context(|| format!("opening audio input {name:?}"))?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // The audio callback only downmixes and queues samples; everything else
    // happens on the analysis thread. Pitch detection takes milliseconds and
    // must not run inside a real-time callback.
    let queue = Arc::new(Mutex::new(VecDeque::with_capacity(QUEUE_SAMPLES)));

    let stream = (match sample_format {
        cpal::SampleFormat::I8 => build_stream::<i8>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::U8 => build_stream::<u8>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::U32 => build_stream::<u32>(&device, &config, channels, &queue, &events),
        cpal::SampleFormat::F64 => build_stream::<f64>(&device, &config, channels, &queue, &events),
        other => bail!("unsupported audio sample format {other}"),
    })?;
    stream.play().context("starting the audio input")?;

    let running = Arc::new(AtomicBool::new(true));
    let analysis = {
        let queue = Arc::clone(&queue);
        let running = Arc::clone(&running);
        let events = events.clone();
        std::thread::Builder::new()
            .name("scx2-tuner-dsp".into())
            .spawn(move || analyse(queue, running, events, sample_rate))
            .context("starting the pitch analysis thread")?
    };
    let _ = events.try_send(Event::Running(name));

    loop {
        match stop.recv_timeout(Duration::from_millis(100)) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    running.store(false, Ordering::Relaxed);
    drop(stream);
    let _ = analysis.join();
    Ok(())
}

/// Prefer the amp's capture endpoint, then the requested device, then default.
fn choose_device(
    inputs: &[(String, cpal::Device)],
    wanted: Option<&str>,
    default: Option<cpal::Device>,
) -> usize {
    if let Some(wanted) = wanted {
        if let Some(index) = inputs.iter().position(|(name, _)| name == wanted) {
            return index;
        }
    }

    if let Some(index) = inputs.iter().position(|(name, _)| is_amp_input(name)) {
        return index;
    }

    if let Some(default) = default {
        if let Ok(name) = default.name() {
            if let Some(index) = inputs.iter().position(|(candidate, _)| candidate == &name) {
                return index;
            }
        }
    }
    0
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
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
    let mut block: Vec<f32> = Vec::new();
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            block.clear();
            for frame in data.chunks(channels) {
                let mono = frame
                    .iter()
                    .map(|sample| f32::from_sample(*sample))
                    .sum::<f32>()
                    / frame.len().max(1) as f32;
                if mono.is_finite() {
                    block.push(mono);
                }
            }
            push_samples(&queue, &block);
        },
        move |error| {
            let _ = errors.try_send(Event::Error(format!("audio stream error: {error}")));
        },
        None,
    )?;
    Ok(stream)
}

/// Hand a block of samples to the analysis thread. `try_lock` keeps the audio
/// callback real-time: if the queue is busy the block is dropped rather than
/// blocking the stream.
fn push_samples(queue: &Mutex<VecDeque<f32>>, samples: &[f32]) {
    let Ok(mut queue) = queue.try_lock() else {
        return;
    };
    let overflow = (queue.len() + samples.len()).saturating_sub(QUEUE_SAMPLES);
    for _ in 0..overflow.min(queue.len()) {
        queue.pop_front();
    }
    queue.extend(samples.iter().copied());
}

/// Pitch detection runs here, away from the audio callback.
fn analyse(
    queue: Arc<Mutex<VecDeque<f32>>>,
    running: Arc<AtomicBool>,
    events: SyncSender<Event>,
    sample_rate: f32,
) {
    let mut tracker = PitchTracker::new(sample_rate);
    let mut scratch: Vec<f32> = Vec::with_capacity(4096);
    while running.load(Ordering::Relaxed) {
        scratch.clear();
        if let Ok(mut queue) = queue.lock() {
            scratch.extend(queue.drain(..));
        }
        if scratch.is_empty() {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        for sample in &scratch {
            if let Some(analysis) = tracker.push(*sample) {
                let _ = events.try_send(Event::Reading(analysis));
            }
        }
    }
}

/// Streaming YIN pitch detector with fixed buffers, safe to run in a callback.
pub struct PitchTracker {
    sample_rate: f32,
    window: usize,
    hop: usize,
    samples: VecDeque<f32>,
    work: Vec<f32>,
    difference: Vec<f32>,
    cumulative: Vec<f32>,
    min_lag: usize,
    max_lag: usize,
    /// Hann window, so the string probes do not leak between harmonics.
    hann: Vec<f32>,
    probes: [StringProbe; 6],
}

/// One open string tracked by a narrow-band DFT. Comparing the phase of two
/// consecutive windows gives the frequency offset well below one FFT bin.
struct StringProbe {
    frequency: f32,
    /// Phase of the previous window, or `None` while the string is quiet.
    phase: Option<f32>,
    /// Smoothed last estimate, so the display does not flicker.
    cents: f32,
}

impl PitchTracker {
    pub fn new(sample_rate: f32) -> Self {
        let window = ((sample_rate * WINDOW_SECONDS) as usize).clamp(2048, 16_384);
        let hop = (window / 2).max(1);
        let min_lag = ((sample_rate / MAX_FREQ).floor() as usize).max(1);
        let max_lag = ((sample_rate / MIN_FREQ).ceil() as usize).clamp(min_lag + 1, window / 2);
        let hann = (0..window)
            .map(|index| {
                let phase = std::f32::consts::TAU * index as f32 / (window.max(2) - 1) as f32;
                0.5 - 0.5 * phase.cos()
            })
            .collect();
        PitchTracker {
            sample_rate,
            window,
            hop,
            samples: VecDeque::with_capacity(window),
            work: Vec::with_capacity(window),
            difference: vec![0.0; max_lag + 2],
            cumulative: vec![0.0; max_lag + 2],
            min_lag,
            max_lag,
            hann,
            probes: STRINGS.map(|(_, frequency)| StringProbe {
                frequency,
                phase: None,
                cents: 0.0,
            }),
        }
    }

    /// Feed one mono sample. Returns an analysis when a full window is ready.
    pub fn push(&mut self, sample: f32) -> Option<Analysis> {
        if !sample.is_finite() {
            return None;
        }
        if self.samples.len() == self.window {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        if self.samples.len() < self.window {
            return None;
        }

        let analysis = self.analyze();
        for _ in 0..self.hop {
            self.samples.pop_front();
        }
        Some(analysis)
    }

    fn analyze(&mut self) -> Analysis {
        self.work.clear();
        self.work.extend(self.samples.iter().copied());

        // Remove DC before both RMS and pitch analysis. The amp's USB output
        // can carry a small offset, which biases the difference function.
        let mean = self.work.iter().sum::<f32>() / self.work.len() as f32;
        for sample in &mut self.work {
            *sample -= mean;
        }
        let level = (self.work.iter().map(|sample| sample * sample).sum::<f32>()
            / self.work.len() as f32)
            .sqrt();
        let frequency = if level >= SILENCE { self.yin() } else { None };
        let strings = analyse_strings(
            &self.work,
            &self.hann,
            &mut self.probes,
            self.sample_rate,
            self.hop,
            level,
        );
        Analysis {
            reading: Reading { frequency, level },
            strings,
        }
    }

    fn yin(&mut self) -> Option<f32> {
        let compare = self.work.len().saturating_sub(self.max_lag);
        if compare < self.min_lag * 2 {
            return None;
        }

        for tau in 1..=self.max_lag {
            let mut sum = 0.0;
            for index in 0..compare {
                let delta = self.work[index] - self.work[index + tau];
                sum += delta * delta;
            }
            self.difference[tau] = sum;
        }

        let mut running = 0.0;
        self.cumulative[0] = 1.0;
        for tau in 1..=self.max_lag {
            running += self.difference[tau];
            self.cumulative[tau] = if running > 0.0 {
                self.difference[tau] * tau as f32 / running
            } else {
                1.0
            };
        }

        let mut tau = self.min_lag;
        while tau <= self.max_lag {
            if self.cumulative[tau] < YIN_THRESHOLD {
                while tau + 1 <= self.max_lag && self.cumulative[tau + 1] < self.cumulative[tau] {
                    tau += 1;
                }
                let period = tau as f32 + parabolic_offset(&self.cumulative, tau);
                if period > 0.0 {
                    return Some(self.sample_rate / period);
                }
                return None;
            }
            tau += 1;
        }
        None
    }
}

/// Analyse all six open strings at once.
///
/// Each string gets a narrow-band DFT at its open frequency. Its magnitude says
/// whether the string is sounding; the phase advance between two windows says
/// how far it is from pitch, at a resolution far finer than one DFT bin.
fn analyse_strings(
    work: &[f32],
    hann: &[f32],
    probes: &mut [StringProbe; 6],
    sample_rate: f32,
    hop: usize,
    level: f32,
) -> [StringReading; 6] {
    let mut readings = [StringReading::default(); 6];
    let normalize = 2.0 / hann.iter().sum::<f32>().max(1e-6);
    let floor = (level * 0.15).max(0.0025);
    let hop = hop.max(1) as f32;

    for (index, probe) in probes.iter_mut().enumerate() {
        let (re, im) = dft(work, hann, probe.frequency, sample_rate);
        let fundamental = (re * re + im * im).sqrt() * normalize;
        let (re2, im2) = dft(work, hann, probe.frequency * 2.0, sample_rate);
        let harmonic = (re2 * re2 + im2 * im2).sqrt() * normalize;
        let strength = fundamental + 0.5 * harmonic;

        if strength < floor {
            probe.phase = None;
            continue;
        }

        let phase = im.atan2(re);
        let Some(previous) = probe.phase else {
            // First window of a note: no phase to compare against yet.
            probe.phase = Some(phase);
            probe.cents = 0.0;
            readings[index] = StringReading {
                active: true,
                cents: 0.0,
                level: strength,
            };
            continue;
        };
        probe.phase = Some(phase);

        let advance = wrap_phase(
            phase - previous - std::f32::consts::TAU * probe.frequency * hop / sample_rate,
        );
        let offset = advance * sample_rate / (std::f32::consts::TAU * hop);
        let raw = 1200.0 * (offset / probe.frequency + 1.0).log2();
        if raw.abs() > 90.0 {
            // Nearly a semitone out: some other note is ringing here.
            continue;
        }
        probe.cents = probe.cents * 0.45 + raw * 0.55;
        readings[index] = StringReading {
            active: true,
            cents: probe.cents.clamp(-50.0, 50.0),
            level: strength,
        };
    }
    readings
}

/// Real and imaginary parts of the DFT at `frequency`, Hann-windowed.
fn dft(work: &[f32], hann: &[f32], frequency: f32, sample_rate: f32) -> (f32, f32) {
    let step = -std::f64::consts::TAU * frequency as f64 / sample_rate as f64;
    let (step_re, step_im) = (step.cos(), step.sin());
    let (mut re, mut im) = (1.0f64, 0.0f64);
    let (mut sum_re, mut sum_im) = (0.0f64, 0.0f64);
    for (sample, window) in work.iter().zip(hann) {
        let value = *sample as f64 * *window as f64;
        sum_re += value * re;
        sum_im += value * im;
        let next_re = re * step_re - im * step_im;
        im = re * step_im + im * step_re;
        re = next_re;
    }
    (sum_re as f32, sum_im as f32)
}

/// Fold a phase into `(-pi, pi]`.
fn wrap_phase(phase: f32) -> f32 {
    let wrapped = phase.rem_euclid(std::f32::consts::TAU);
    if wrapped > std::f32::consts::PI {
        wrapped - std::f32::consts::TAU
    } else {
        wrapped
    }
}

/// Sub-sample peak using the three points around `tau`.
fn parabolic_offset(values: &[f32], tau: usize) -> f32 {
    if tau == 0 || tau + 1 >= values.len() {
        return 0.0;
    }
    let left = values[tau - 1];
    let middle = values[tau];
    let right = values[tau + 1];
    let denominator = left - 2.0 * middle + right;
    if denominator.abs() < 1e-9 {
        0.0
    } else {
        (0.5 * (left - right) / denominator).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_amp_input_is_recognised_on_every_platform() {
        // Windows names the endpoint after the jack; ALSA after the card id.
        assert!(is_amp_input("Internal AUX Jack (SuperChamp X2 Amp)"));
        assert!(is_amp_input("hw:CARD=Amp,DEV=0"));
        assert!(is_amp_input("plughw:CARD=Amp,DEV=0"));
        assert!(!is_amp_input("hw:CARD=Generic_1,DEV=0"));
        assert!(!is_amp_input("default"));
    }

    fn sine(frequency: f32, sample_rate: f32, seconds: f32) -> Vec<f32> {
        let count = (sample_rate * seconds) as usize;
        (0..count)
            .map(|index| {
                let phase = std::f32::consts::TAU * frequency * index as f32 / sample_rate;
                0.35 * phase.sin()
            })
            .collect()
    }

    fn last_frequency(samples: &[f32], sample_rate: f32) -> Option<Analysis> {
        let mut tracker = PitchTracker::new(sample_rate);
        let mut last = None;
        for sample in samples {
            if let Some(analysis) = tracker.push(*sample) {
                last = Some(analysis);
            }
        }
        last
    }

    /// A chord made of detuned sines, each with a second harmonic.
    fn chord(frequencies: &[(f32, f32)], sample_rate: f32, seconds: f32) -> Vec<f32> {
        let count = (sample_rate * seconds) as usize;
        (0..count)
            .map(|index| {
                let time = index as f32 / sample_rate;
                frequencies
                    .iter()
                    .map(|(frequency, cents)| {
                        let frequency = frequency * 2.0f32.powf(cents / 1200.0);
                        let phase = std::f32::consts::TAU * frequency * time;
                        0.2 * phase.sin() + 0.08 * (2.0 * phase).sin()
                    })
                    .sum()
            })
            .collect()
    }

    #[test]
    fn detects_concert_a() {
        let analysis = last_frequency(&sine(440.0, 48_000.0, 0.5), 48_000.0).unwrap();
        let frequency = analysis.reading.frequency.expect("a pitch");
        assert!(
            (frequency - 440.0).abs() < 1.0,
            "detected {frequency} instead of 440"
        );
    }

    #[test]
    fn detects_low_e() {
        let analysis = last_frequency(&sine(82.4069, 48_000.0, 0.7), 48_000.0).unwrap();
        let frequency = analysis.reading.frequency.expect("a pitch");
        assert!(
            (frequency - 82.4069).abs() < 0.8,
            "detected {frequency} instead of 82.4"
        );
    }

    #[test]
    fn silence_is_not_a_note() {
        let analysis = last_frequency(&vec![0.0; 10_000], 48_000.0).unwrap();
        assert_eq!(analysis.reading.frequency, None);
        assert_eq!(analysis.reading.level, 0.0);
        assert!(analysis.strings.iter().all(|string| !string.active));
    }

    #[test]
    fn polyphonic_chord_lights_the_strings_that_ring() {
        // Low E, A and D, each eight cents sharp, as a three-string chord.
        let samples = chord(
            &[
                (STRINGS[0].1, 8.0),
                (STRINGS[1].1, 8.0),
                (STRINGS[2].1, 8.0),
            ],
            48_000.0,
            0.6,
        );
        let analysis = last_frequency(&samples, 48_000.0).unwrap();
        for index in 0..3 {
            let string = analysis.strings[index];
            assert!(string.active, "{} should be sounding", STRINGS[index].0);
            assert!(
                (string.cents - 8.0).abs() < 3.0,
                "{} read {:.1} cents, expected 8",
                STRINGS[index].0,
                string.cents
            );
        }
        for index in 3..6 {
            assert!(
                !analysis.strings[index].active,
                "{} should be quiet",
                STRINGS[index].0
            );
        }
    }

    #[test]
    fn polyphonic_chord_reports_in_tune_strings() {
        let all: Vec<(f32, f32)> = STRINGS
            .iter()
            .map(|(_, frequency)| (*frequency, 0.0))
            .collect();
        let samples = chord(&all, 48_000.0, 0.6);
        let analysis = last_frequency(&samples, 48_000.0).unwrap();
        for (index, string) in analysis.strings.iter().enumerate() {
            assert!(string.active, "{} should be sounding", STRINGS[index].0);
            assert!(
                string.cents.abs() < 3.0,
                "{} read {:.1} cents",
                STRINGS[index].0,
                string.cents
            );
        }
    }

    #[test]
    fn flat_string_reads_negative_cents() {
        let samples = chord(&[(STRINGS[4].1, -12.0)], 48_000.0, 0.6);
        let analysis = last_frequency(&samples, 48_000.0).unwrap();
        let b = analysis.strings[4];
        assert!(b.active, "B should be sounding");
        assert!(
            (b.cents + 12.0).abs() < 3.0,
            "B read {:.1} cents, expected -12",
            b.cents
        );
    }

    #[test]
    fn nearest_note_and_cents_are_centred_on_a4() {
        let note = note_for(440.0).unwrap();
        assert_eq!(note.name, "A");
        assert_eq!(note.octave, 4);
        assert!(note.cents.abs() < 0.001);

        let sharp = note_for(446.0).unwrap();
        assert_eq!(sharp.name, "A");
        assert!(sharp.cents > 0.0 && sharp.cents < 50.0);

        let low_e = note_for(82.4069).unwrap();
        assert_eq!((low_e.name, low_e.octave), ("E", 2));
    }
}
