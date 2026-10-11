//! Microphone capture: device audio → mono → 16 kHz → s16le chunks.
//! Port of AudioCaptureService.swift / AudioInputDeviceService.swift.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::soniox::SAMPLE_RATE;

pub fn input_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = host
        .input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

/// Box-filter resampler: each output sample is the mean of the input samples it covers,
/// which doubles as a simple anti-alias filter. Keeps state across chunks.
pub struct Resampler {
    ratio: f64,
    buf: Vec<f32>,
    pos: f64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        Self { ratio: in_rate as f64 / out_rate as f64, buf: Vec::new(), pos: 0.0 }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<i16> {
        self.buf.extend_from_slice(input);
        let mut out = Vec::with_capacity((input.len() as f64 / self.ratio) as usize + 1);
        loop {
            let start = self.pos.floor() as usize;
            let end = ((self.pos + self.ratio).floor() as usize).max(start + 1);
            if end > self.buf.len() {
                break;
            }
            let window = &self.buf[start..end];
            let mean = window.iter().sum::<f32>() / window.len() as f32;
            out.push((mean.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            self.pos += self.ratio;
        }
        let consumed = (self.pos.floor() as usize).min(self.buf.len());
        self.buf.drain(..consumed);
        self.pos -= consumed as f64;
        out
    }
}

pub struct AudioCapture {
    stop_tx: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl AudioCapture {
    /// Starts capturing on `device_name` (or the default input). Returns once the stream
    /// is running or failed to start. The stream (and the chunk callback) is dropped on `stop`.
    pub fn start(
        device_name: Option<String>,
        on_chunk: impl FnMut(Vec<u8>) + Send + 'static,
        on_level: impl Fn(f32) + Send + 'static,
    ) -> Result<Self, String> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let thread = std::thread::spawn(move || {
            let stream = match build_stream(device_name, on_chunk, on_level) {
                Ok(stream) => stream,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            let _ = stop_rx.recv();
            drop(stream);
        });
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self { stop_tx, thread: Some(thread) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("timed out opening the microphone".into()),
        }
    }

    pub fn stop(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn find_device(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(name) = name {
        if let Some(device) = host
            .input_devices()
            .ok()
            .and_then(|mut devices| devices.find(|d| d.name().ok().as_deref() == Some(name)))
        {
            return Some(device);
        }
        crate::log::write(&format!("microphone '{name}' not found, using default input"));
    }
    host.default_input_device()
}

fn build_stream(
    device_name: Option<String>,
    on_chunk: impl FnMut(Vec<u8>) + Send + 'static,
    on_level: impl Fn(f32) + Send + 'static,
) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = find_device(&host, device_name.as_deref())
        .ok_or_else(|| "no microphone found".to_string())?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    crate::log::write(&format!(
        "microphone '{}': {} Hz, {} ch, {format}",
        device.name().unwrap_or_default(),
        config.sample_rate.0,
        config.channels
    ));
    match format {
        SampleFormat::F32 => input_stream::<f32>(&device, &config, on_chunk, on_level),
        SampleFormat::I16 => input_stream::<i16>(&device, &config, on_chunk, on_level),
        SampleFormat::I32 => input_stream::<i32>(&device, &config, on_chunk, on_level),
        SampleFormat::U16 => input_stream::<u16>(&device, &config, on_chunk, on_level),
        other => Err(format!("unsupported sample format {other}")),
    }
}

/// Logs the loudest sample of a recording when the stream (and its callback) is dropped.
struct PeakLog(f32);

impl Drop for PeakLog {
    fn drop(&mut self) {
        crate::log::write(&format!("microphone peak level {:.3} (1.0 = full scale)", self.0));
    }
}

fn input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut on_chunk: impl FnMut(Vec<u8>) + Send + 'static,
    on_level: impl Fn(f32) + Send + 'static,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let mut resampler = Resampler::new(config.sample_rate.0, SAMPLE_RATE);
    let mut mono = Vec::<f32>::new();
    let mut last_level = Instant::now();
    let mut peak = PeakLog(0.0);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                mono.clear();
                mono.extend(data.chunks(channels).map(|frame| {
                    frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32
                }));
                // Borrow the whole guard so the closure owns it (not just the f32 field),
                // and it is dropped, and logged, with the stream.
                let peak = &mut peak;
                peak.0 = mono.iter().fold(peak.0, |p, s| p.max(s.abs()));
                if last_level.elapsed() >= Duration::from_millis(50) && !mono.is_empty() {
                    last_level = Instant::now();
                    let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
                    on_level((rms * 5.0).min(1.0));
                }
                let samples = resampler.process(&mono);
                if !samples.is_empty() {
                    on_chunk(samples.iter().flat_map(|s| s.to_le_bytes()).collect());
                }
            },
            |e| crate::log::write(&format!("audio stream error: {e}")),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsamples_48k_to_16k() {
        let mut r = Resampler::new(48_000, 16_000);
        let mut total = 0;
        for _ in 0..100 {
            total += r.process(&vec![0.5; 480]).len();
        }
        assert_eq!(total, 16_000);
    }

    #[test]
    fn handles_fractional_ratio() {
        let mut r = Resampler::new(44_100, 16_000);
        let total: usize = (0..100).map(|_| r.process(&vec![0.0; 441]).len()).sum();
        assert!((15_998..=16_000).contains(&total), "got {total}");
    }

    #[test]
    fn passthrough_at_16k() {
        let mut r = Resampler::new(16_000, 16_000);
        let out = r.process(&[0.0, 1.0, -1.0]);
        assert_eq!(out, vec![0, i16::MAX, -i16::MAX]);
    }
}
