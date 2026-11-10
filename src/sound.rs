//! Short sounds made up at start and mixed as they play, so that many can sound at once: dice shaken and striking
//! the board, checkers set down, knocked off or dropped into the tray, the checkers set out for a new game, and the
//! interface.

use std::f64::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const RATE: f64 = 44100.0;
const MAX_VOICES: usize = 40;
const LEVEL: f32 = 30000.0 / 32768.0; // of the loudest sample let out

#[derive(Clone, Copy)]
pub enum Effect {
    Rattle,
    Die,
    Checker,
    Hit,
    Tray,
    Setup,
    Click,
    NoMove,
    Win,
}

type Samples = Vec<f32>;

fn silence(seconds: f64) -> Samples {
    vec![0.0; (seconds * RATE) as usize]
}

fn add(target: &mut Samples, source: &[f32], at: f64, gain: f32) {
    let offset = (at * RATE) as usize;
    if target.len() < offset + source.len() {
        target.resize(offset + source.len(), 0.0);
    }
    for (sum, sample) in target[offset..].iter_mut().zip(source) {
        *sum += sample * gain;
    }
}

/// Numbers out of a small generator, the same every time for a seed.
struct Noise(u64);

impl Noise {
    /// White noise from -1 to 1.
    fn next(&mut self) -> f32 {
        const MODULUS: u64 = 2_147_483_647;
        self.0 = self.0 * 48_271 % MODULUS;
        (self.0 - 1) as f32 / (MODULUS - 2) as f32 * 2.0 - 1.0
    }

    /// From 0 to 1.
    fn unit(&mut self) -> f64 {
        f64::from(self.next()) * 0.5 + 0.5
    }
}

/// A resonance of a struck body: its frequency in Hz, its amplitude, and the seconds it takes to fall to 1/e.
type Mode = (f64, f64, f64);

/// A struck body: a burst of noise, the higher part of it for a hard click, and damped resonances.
fn strike(length: f64, modes: &[Mode], click: f64, click_decay: f64, brightness: f64, seed: u64) -> Samples {
    let mut samples = silence(length);
    let mut noise = Noise(seed);
    let mut low = 0.0f32;
    for (i, sample) in samples.iter_mut().enumerate() {
        let t = i as f64 / RATE;
        let white = noise.next();
        low += (white - low) * 0.08;
        let bright = white - low;
        let burst = click * (brightness * f64::from(bright) + (1.0 - brightness) * 3.0 * f64::from(low)) * (-t / click_decay).exp();
        let ring: f64 = modes.iter().map(|(frequency, gain, decay)| gain * (TAU * frequency * t).sin() * (-t / decay).exp()).sum();
        *sample = ((burst + ring) * (t / 0.0003).min(1.0)) as f32;
    }
    samples
}

/// A small bell: inharmonic partials dying away at their own rates.
fn bell(frequency: f64, length: f64) -> Samples {
    // The ratio to the bell's frequency, the gain and the decay of each partial.
    const PARTIALS: [(f64, f64, f64); 4] = [(1.0, 1.0, 1.0), (2.0, 0.45, 0.6), (2.76, 0.3, 0.4), (5.4, 0.12, 0.25)];
    let mut samples = silence(length);
    for (i, sample) in samples.iter_mut().enumerate() {
        let t = i as f64 / RATE;
        let value: f64 = PARTIALS.iter().map(|(ratio, gain, decay)| gain * (TAU * frequency * ratio * t).sin() * (-t / (length * 0.35 * decay)).exp()).sum();
        *sample = (value * (t / 0.004).min(1.0)) as f32;
    }
    samples
}

/// Two hard little bodies knocking together: a die on a die, a checker on a checker.
fn knock(seed: u64, pitch: f64) -> Samples {
    strike(0.07, &[(2900.0 * pitch, 0.8, 0.004), (4700.0 * pitch, 0.45, 0.002), (1700.0 * pitch, 0.3, 0.006)], 1.3, 0.0006, 0.85, seed)
}

/// The samples scaled so that the loudest is `peak`.
fn wave(samples: Samples, peak: f32) -> Arc<[f32]> {
    let loudest = samples.iter().fold(1e-6f32, |loudest, sample| loudest.max(sample.abs()));
    samples.into_iter().map(|sample| sample * peak / loudest).collect()
}

/// A sound being played: how far it has got, in samples of the sound, how fast it goes and how loud it is in each ear.
struct Voice {
    wave: Arc<[f32]>,
    position: f64,
    rate: f64,
    left: f32,
    right: f32,
}

impl Voice {
    fn ended(&self) -> bool {
        self.position as usize + 1 >= self.wave.len()
    }
}

type Voices = Arc<Mutex<Vec<Voice>>>;

/// The stream to the default output device, which mixes the voices as it asks for more.
struct Output {
    _stream: cpal::Stream,
    voices: Voices,
    broken: Arc<AtomicBool>,
}

impl Output {
    fn open() -> Option<Output> {
        let device = cpal::default_host().default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let format = supported.sample_format();
        let config = supported.config();
        let voices = Voices::default();
        let broken = Arc::new(AtomicBool::new(false));
        let stream = match format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, &voices, &broken),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, &voices, &broken),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, &voices, &broken),
            cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, &voices, &broken),
            cpal::SampleFormat::U8 => build_stream::<u8>(&device, &config, &voices, &broken),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Output { _stream: stream, voices, broken })
    }
}

fn build_stream<T>(device: &cpal::Device, config: &cpal::StreamConfig, voices: &Voices, broken: &Arc<AtomicBool>) -> Option<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    let step = RATE / f64::from(config.sample_rate.0);
    let voices = voices.clone();
    let broken = broken.clone();
    let mut mix: Vec<[f32; 2]> = Vec::new();
    let fill = move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
        mix.clear();
        mix.resize(output.len() / channels, [0.0; 2]);
        {
            let mut voices = voices.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for voice in voices.iter_mut() {
                for frame in &mut mix {
                    if voice.ended() {
                        break;
                    }
                    // The sound's rate is turned into the device's by taking the samples between its own.
                    let index = voice.position as usize;
                    let fraction = (voice.position - index as f64) as f32;
                    let sample = voice.wave[index] + (voice.wave[index + 1] - voice.wave[index]) * fraction;
                    frame[0] += sample * voice.left;
                    frame[1] += sample * voice.right;
                    voice.position += voice.rate * step;
                }
            }
            voices.retain(|voice| !voice.ended());
        }
        // A gentle limit, so that a crowd of knocks does not crackle.
        let limited = |x: f32| T::from_sample((x / (1.0 + x.abs() * 0.35)).clamp(-1.0, 1.0) * LEVEL);
        for (frame, [left, right]) in output.chunks_mut(channels).zip(&mix) {
            if let [first, second, rest @ ..] = frame {
                *first = limited(*left);
                *second = limited(*right);
                rest.fill(T::from_sample(0.0));
            } else {
                frame.fill(limited((left + right) / 2.0));
            }
        }
    };
    device.build_output_stream(config, fill, move |_| broken.store(true, Ordering::Relaxed), None).ok()
}

pub struct SoundBank {
    pub enabled: bool,
    waves: [Arc<[f32]>; 9],
    output: Option<Output>,
    opened: Instant,
}

impl SoundBank {
    pub fn new() -> SoundBank {
        let mut random = Noise(23);

        // Dice shaken in the hand as they leave it: quick knocks of bone on bone.
        let mut rattle = silence(0.3);
        for i in 0..7 {
            let pitch = 0.85 + 0.35 * random.unit();
            let at = 0.025 * i as f64 + 0.015 * random.unit();
            add(&mut rattle, &knock(40 + i, pitch), at, (0.3 + 0.6 * random.unit()) as f32);
        }

        // A lone checker knocked off: the clack of checker on checker over the knock on the board.
        let mut hit = strike(0.14, &[(980.0, 0.5, 0.009), (440.0, 0.35, 0.018)], 0.7, 0.0012, 0.5, 9);
        add(&mut hit, &knock(11, 0.62), 0.0, 1.3);

        // The checkers set out for a new game: one after another onto the board.
        let mut setup = silence(0.6);
        for i in 0..10 {
            let checker = strike(0.1, &[(980.0 * (0.92 + 0.16 * random.unit()), 0.6, 0.008), (440.0, 0.4, 0.016)], 0.7, 0.001, 0.5, 60 + i);
            let at = 0.045 * i as f64 + 0.02 * random.unit();
            add(&mut setup, &checker, at, (0.35 + 0.5 * random.unit()) as f32);
        }

        let mut click = silence(0.03);
        for (i, sample) in click.iter_mut().enumerate() {
            let t = i as f64 / RATE;
            *sample = ((TAU * 2600.0 * t).sin() * (-t / 0.004).exp()) as f32;
        }

        let mut no_move = silence(0.9);
        add(&mut no_move, &bell(392.0, 0.6), 0.0, 1.0);
        add(&mut no_move, &bell(311.1, 0.7), 0.16, 1.0);

        let mut chord = silence(1.8);
        for (i, note) in [523.25, 659.25, 783.99, 1046.5].into_iter().enumerate() {
            add(&mut chord, &bell(note, 1.5), i as f64 * 0.11, 0.8);
        }

        // In the order of `Effect`.
        let waves = [
            wave(rattle, 0.35),
            // A die striking the board: a hard click and the wood answering under it.
            wave(strike(0.13, &[(2150.0, 0.55, 0.004), (3900.0, 0.3, 0.002), (760.0, 0.45, 0.014), (330.0, 0.35, 0.024)], 1.1, 0.0008, 0.7, 5), 0.6),
            // A checker set down on the board: a short, woody knock.
            wave(strike(0.14, &[(980.0, 0.6, 0.009), (1720.0, 0.32, 0.004), (440.0, 0.42, 0.018), (260.0, 0.25, 0.026)], 0.8, 0.0012, 0.5, 7), 0.55),
            wave(hit, 0.65),
            // A checker dropped into the felt of the tray: a dull thud.
            wave(strike(0.2, &[(190.0, 0.8, 0.022), (360.0, 0.4, 0.012), (760.0, 0.15, 0.006)], 0.45, 0.006, 0.15, 13), 0.5),
            wave(setup, 0.4),
            wave(click, 0.18),
            wave(no_move, 0.2),
            wave(chord, 0.35),
        ];
        SoundBank { enabled: true, waves, output: Output::open(), opened: Instant::now() }
    }

    /// Plays a sound as it was made.
    pub fn play(&mut self, effect: Effect) {
        self.play_at(effect, 1.0, 0.0, 1.0);
    }

    /// Plays a sound at a `volume` from 0 to 1, a `pan` from -1 (left) to 1 (right), and a `pitch` as a rate of playback.
    pub fn play_at(&mut self, effect: Effect, volume: f32, pan: f32, pitch: f32) {
        if !self.enabled || volume <= 0.001 {
            return;
        }

        // A device that went away, or was not there, is looked for again, though not at every sound.
        let lost = self.output.as_ref().is_none_or(|output| output.broken.load(Ordering::Relaxed));
        if lost && self.opened.elapsed() > Duration::from_secs(3) {
            self.output = Output::open();
            self.opened = Instant::now();
        }

        if let Some(output) = &self.output {
            let pan = pan.clamp(-1.0, 1.0);
            let voice = Voice {
                wave: self.waves[effect as usize].clone(),
                position: 0.0,
                rate: f64::from(pitch.clamp(0.25, 4.0)),
                left: volume * (1.0 - pan).min(1.0),
                right: volume * (1.0 + pan).min(1.0),
            };
            let mut voices = output.voices.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if voices.len() >= MAX_VOICES {
                voices.remove(0);
            }
            voices.push(voice);
        }
    }
}
