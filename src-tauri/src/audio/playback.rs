use anyhow::Result;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::sync::Arc;

pub struct AudioPlayer {
    _stream: OutputStream,
    sink: Arc<Sink>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let (stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        Ok(Self {
            _stream: stream,
            sink: Arc::new(sink),
        })
    }

    pub fn play(&self, samples: &[f32], sample_rate: u32) -> Result<()> {
        let source = SamplesBuffer::new(1, sample_rate, samples.to_vec());
        self.sink.append(source);
        Ok(())
    }

    pub fn play_and_wait(&self, samples: &[f32], sample_rate: u32) -> Result<()> {
        self.play(samples, sample_rate)?;
        self.sink.sleep_until_end();
        Ok(())
    }

    pub fn stop(&self) {
        self.sink.stop();
    }

    pub fn pause(&self) {
        self.sink.pause();
    }

    pub fn resume(&self) {
        self.sink.play();
    }

    pub fn set_volume(&self, volume: f32) {
        self.sink.set_volume(volume);
    }

    pub fn set_speed(&self, speed: f32) {
        self.sink.set_speed(speed);
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }
}
