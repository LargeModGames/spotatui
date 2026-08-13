// PipeWire-native audio capture for Linux
// This provides direct access to PipeWire's audio graph for monitor capture

use super::analyzer::{create_shared_analyzer, spectrum_for, SharedAnalyzer, SpectrumData};
use pipewire as pw;
use pw::spa::param::audio::AudioInfoRaw;
use pw::spa::pod::Pod;
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

/// Manages audio capture from PipeWire sink monitors
pub struct PipeWireCapture {
  analyzer: SharedAnalyzer,
  active: Arc<AtomicBool>,
  _thread: thread::JoinHandle<()>,
}

impl PipeWireCapture {
  /// Create a new PipeWire audio capture manager sized for `display_bars`
  /// Returns None if PipeWire initialization fails
  pub fn new(display_bars: usize) -> Option<Self> {
    // Built at the rate we request below; the param_changed callback swaps in
    // the rate PipeWire actually negotiates.
    let analyzer = create_shared_analyzer(48000, display_bars);
    let active = Arc::new(AtomicBool::new(true));

    let analyzer_clone = analyzer.clone();
    let active_clone = active.clone();

    // PipeWire requires its own thread with a main loop
    let thread = thread::spawn(move || {
      if let Err(e) = run_pipewire_capture(analyzer_clone, active_clone) {
        eprintln!("[audio-viz] PipeWire capture error: {:?}", e);
      }
    });

    // Give PipeWire a moment to initialize
    std::thread::sleep(std::time::Duration::from_millis(300));

    if active.load(Ordering::Relaxed) {
      Some(Self {
        analyzer,
        active,
        _thread: thread,
      })
    } else {
      None
    }
  }

  /// Get the current spectrum data, sized to `desired_bars` (the analyzer
  /// rebuilds its plan when the count changes, and clamps unsupportable counts)
  pub fn get_spectrum(&self, desired_bars: usize) -> Option<SpectrumData> {
    if !self.active.load(Ordering::Relaxed) {
      return None;
    }

    spectrum_for(&self.analyzer, desired_bars)
  }

  /// Check if audio capture is currently active
  pub fn is_active(&self) -> bool {
    self.active.load(Ordering::Relaxed)
  }
}

impl Drop for PipeWireCapture {
  fn drop(&mut self) {
    self.active.store(false, Ordering::Relaxed);
  }
}

/// User data passed to the stream callbacks
#[derive(Default)]
struct StreamData {
  channels: AtomicU32,
  format: std::sync::Mutex<AudioInfoRaw>,
}

fn run_pipewire_capture(
  analyzer: SharedAnalyzer,
  active: Arc<AtomicBool>,
) -> Result<(), pw::Error> {
  // pipewire 0.9+ no longer has a separate init() call - initialization happens in MainLoopBox::new()
  let mainloop = pw::main_loop::MainLoopBox::new(None)?;
  let context = pw::context::ContextBox::new(mainloop.loop_(), None)?;
  let core = context.connect(None)?;

  // Properties for audio capture from sink monitors
  let props = pw::properties::properties! {
    *pw::keys::MEDIA_TYPE => "Audio",
    *pw::keys::MEDIA_CATEGORY => "Capture",
    *pw::keys::MEDIA_ROLE => "Music",
    // This is the key property - capture from sink monitor ports!
    *pw::keys::STREAM_CAPTURE_SINK => "true",
  };

  let stream = pw::stream::StreamBox::new(&core, "spotatui-audio-viz", props)?;

  let active_clone = active.clone();
  let analyzer_clone = analyzer.clone();
  let param_analyzer = analyzer.clone();

  // Set up stream listener
  let _listener = stream
    .add_local_listener_with_user_data(StreamData::default())
    .param_changed(move |_, user_data, id, param| {
      let Some(param) = param else { return };
      if id != pw::spa::param::ParamType::Format.as_raw() {
        return;
      }

      // Parse the format to get channel count and negotiated sample rate
      if let Ok(mut format) = user_data.format.lock() {
        if format.parse(param).is_ok() {
          let channels = format.channels();
          user_data.channels.store(channels, Ordering::Relaxed);

          let rate = format.rate();
          if rate > 0 {
            if let Ok(mut analyzer) = param_analyzer.lock() {
              analyzer.set_sample_rate(rate);
            }
          }
        }
      }
    })
    .process(move |stream, user_data| {
      if !active_clone.load(Ordering::Relaxed) {
        return;
      }

      let Some(mut buffer) = stream.dequeue_buffer() else {
        return;
      };

      let datas = buffer.datas_mut();
      if datas.is_empty() {
        return;
      }

      let data = &mut datas[0];
      let n_channels = user_data.channels.load(Ordering::Relaxed).max(1) as usize;

      // Get the actual size of data in the chunk
      let chunk = data.chunk();
      let n_bytes = chunk.size() as usize;

      if n_bytes == 0 {
        return;
      }

      if let Some(samples_bytes) = data.data() {
        // Only process the valid portion of the buffer
        let valid_bytes = &samples_bytes[..n_bytes.min(samples_bytes.len())];

        // Decode the interleaved f32 stream; the analyzer owns the channel
        // rule (0/1 through, mono duplicated) so it is stated in one place.
        let samples: Vec<f32> = valid_bytes
          .chunks_exact(mem::size_of::<f32>())
          .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap_or([0; 4])))
          .collect();

        if !samples.is_empty() {
          if let Ok(mut analyzer) = analyzer_clone.lock() {
            analyzer.push_frames(&samples, n_channels);
          }
        }
      }
    })
    .register()?;

  // Set up audio format - request F32LE, stereo, 48kHz
  let mut audio_info = AudioInfoRaw::new();
  audio_info.set_format(pw::spa::param::audio::AudioFormat::F32LE);
  audio_info.set_rate(48000);
  audio_info.set_channels(2);

  let obj = pw::spa::pod::Object {
    type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
    id: pw::spa::param::ParamType::EnumFormat.as_raw(),
    properties: audio_info.into(),
  };

  let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
    std::io::Cursor::new(Vec::new()),
    &pw::spa::pod::Value::Object(obj),
  )
  .map_err(|_| pw::Error::CreationFailed)?
  .0
  .into_inner();

  let mut params = [Pod::from_bytes(&values).unwrap()];

  // Connect the stream with RT_PROCESS for real-time processing
  stream.connect(
    pw::spa::utils::Direction::Input,
    None,
    pw::stream::StreamFlags::AUTOCONNECT
      | pw::stream::StreamFlags::MAP_BUFFERS
      | pw::stream::StreamFlags::RT_PROCESS,
    &mut params,
  )?;

  // eprintln!("[audio-viz] PipeWire stream connected (sink monitor)");

  // Run the main loop - this blocks until quit is called
  mainloop.run();

  Ok(())
}
