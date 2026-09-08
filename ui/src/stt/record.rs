use std::{
    io::{self, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};

use cpal::{
    Device, Error, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};

use crate::error::Result;

/// A thread-safe wrapper that allows std::io::Write + Seek on a shared Vec<u8>
pub struct SharedCursor {
    inner: Arc<Mutex<Vec<u8>>>,
    position: u64,
}

impl SharedCursor {
    pub fn new(inner: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { inner, position: 0 }
    }
}

impl Write for SharedCursor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "poisoned lock"))?;

        let pos = self.position as usize;
        // If writing past current end, expand the vector
        if pos + buf.len() > guard.len() {
            guard.resize(pos + buf.len(), 0);
        }

        // Copy the bytes into the buffer
        guard[pos..pos + buf.len()].copy_from_slice(buf);
        self.position += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for SharedCursor {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "poisoned lock"))?;
        let len = guard.len() as i64;

        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.position as i64 + offset,
            SeekFrom::End(offset) => len + offset,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek position",
            ));
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }
}

// Not my proudest struct lol
pub struct AudioRecorder {
    device: Device,
    spec: WavSpec,
    sample_format: SampleFormat,
    config: StreamConfig,
    gain: Arc<Mutex<f32>>,

    current_stream: Option<(
        cpal::Stream,
        Arc<Mutex<Option<WavWriter<SharedCursor>>>>,
        Arc<Mutex<Vec<u8>>>,
    )>,
}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable))?;

        let supported_config = device.default_input_config()?;

        let sample_format = supported_config.sample_format();
        let config: cpal::StreamConfig = supported_config.into();

        let spec = WavSpec {
            channels: config.channels,
            sample_rate: config.sample_rate,
            bits_per_sample: 16,
            sample_format: WavSampleFormat::Int,
        };

        Ok(Self {
            device,
            spec,
            sample_format,
            config,
            gain: Arc::new(Mutex::new(1.0)),
            current_stream: None,
        })
    }

    /// Updates the live gain multiplier applied to recorded samples
    /// (see `Settings::mic_gain`). Takes effect immediately, including
    /// mid-recording.
    pub fn set_gain(&self, gain: f32) {
        if let Ok(mut current) = self.gain.lock() {
            *current = gain;
        }
    }
    pub fn start_recording(
        &mut self,
        error_callback: impl Fn() + Send + Sync + 'static,
    ) -> Result<()> {
        if self.current_stream.is_some() {
            return Err(crate::error::ClientError::Audio(Error::new(
                cpal::ErrorKind::DeviceBusy,
            )));
        }

        // 1. Create a heap-allocated, shared byte vector
        let wave_bytes = Arc::new(Mutex::new(Vec::new()));

        // 2. Clone it for the WavWriter wrapper
        let writer_cursor = SharedCursor::new(Arc::clone(&wave_bytes));

        // 3. Initialize WavWriter with your custom thread-safe cursor
        let writer = WavWriter::new(writer_cursor, self.spec)?;
        let writer = Arc::new(Mutex::new(Some(writer)));
        let writer_for_callback = Arc::clone(&writer);

        let err_fn = move |err: cpal::Error| {
            println!("{}", err.kind());
            error_callback();
        };

        let stream = match self.sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(
                &self.device,
                self.config.clone(),
                writer_for_callback,
                self.gain.clone(),
                err_fn,
            )?,
            SampleFormat::I16 => Self::build_stream::<i16>(
                &self.device,
                self.config.clone(),
                writer_for_callback,
                self.gain.clone(),
                err_fn,
            )?,
            SampleFormat::U16 => Self::build_stream::<u16>(
                &self.device,
                self.config.clone(),
                writer_for_callback,
                self.gain.clone(),
                err_fn,
            )?,
            other => {
                return Err(crate::error::ClientError::Generic(format!(
                    "audio error: Unsupported microphone sample format: {other:?}"
                )));
            }
        };

        stream.play()?;

        // 4. Store the stream, writer handle, and the handle to your vector safely
        self.current_stream = Some((stream, writer, wave_bytes));

        Ok(())
    }

    /// Stops the in-progress recording and finalizes the WAV file.
    ///
    /// Takes `&mut self` rather than consuming the recorder: the device,
    /// sample spec and stream config are all cheap to hold onto, and a
    /// hold-to-record shortcut needs to start/stop many times over the
    /// lifetime of one `AudioRecorder` rather than being thrown away
    /// after a single recording.
    pub fn stop_recording(&mut self) -> Result<Vec<u8>> {
        if let Some((stream, writer_lock, wave_bytes_lock)) = self.current_stream.take() {
            drop(stream);

            if let Ok(mut guard) = writer_lock.lock() {
                if let Some(writer) = guard.take() {
                    writer.finalize().unwrap();
                }
            }

            // Return a copy or take ownership of the inner buffer data
            let final_bytes = wave_bytes_lock.lock().unwrap().clone();
            Ok(final_bytes)
        } else {
            Err(crate::error::ClientError::Generic(
                "No active recording found".into(),
            ))
        }
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        writer: Arc<Mutex<Option<WavWriter<SharedCursor>>>>,
        gain: Arc<Mutex<f32>>,
        err_fn: impl Fn(cpal::Error) + Send + 'static,
    ) -> Result<Stream>
    where
        T: cpal::SizedSample + IntoI16Sample,
    {
        Ok(device.build_input_stream(
            config,
            move |data: &[T], _| {
                let Ok(mut guard) = writer.lock() else {
                    return;
                };

                let Some(writer) = guard.as_mut() else {
                    return;
                };

                let gain = gain.lock().map(|g| *g).unwrap_or(1.0);

                for &sample in data {
                    let sample = sample.into_i16_sample();
                    let sample = if gain == 1.0 {
                        sample
                    } else {
                        (sample as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16
                    };

                    // Ignore a write error here. The callback cannot sensibly
                    // return an error to CPAL.
                    let _ = writer.write_sample(sample);
                }
            },
            err_fn,
            None,
        )?)
    }
}

trait IntoI16Sample {
    fn into_i16_sample(self) -> i16;
}

impl IntoI16Sample for i16 {
    #[inline]
    fn into_i16_sample(self) -> i16 {
        self
    }
}

impl IntoI16Sample for f32 {
    #[inline]
    fn into_i16_sample(self) -> i16 {
        let clamped = self.clamp(-1.0, 1.0);

        if clamped >= 0.0 {
            (clamped * i16::MAX as f32) as i16
        } else {
            (clamped * -(i16::MIN as f32)) as i16
        }
    }
}

impl IntoI16Sample for u16 {
    #[inline]
    fn into_i16_sample(self) -> i16 {
        // Convert unsigned 16-bit PCM centered around 32768
        // into signed 16-bit PCM centered around 0.
        (self as i32 - 32768) as i16
    }
}
