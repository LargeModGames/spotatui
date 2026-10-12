use super::*;

/// The parts of Spotify's audio analysis that the pitch view draws, with times in ms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackAnalysis {
  pub sections: Vec<AnalysisSection>,
  pub segments: Vec<AnalysisSegment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisSection {
  pub start_ms: u32,
  pub tempo: f32,
  pub tempo_confidence: f32,
  /// The pitch class of the key, 0 (C) to 11 (B); `None` when no key was detected.
  pub key: Option<u8>,
  pub key_confidence: f32,
  /// `Some(true)` for minor, `Some(false)` for major, `None` when not detected.
  pub minor: Option<bool>,
  pub time_signature: u8,
  pub time_signature_confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisSegment {
  pub start_ms: u32,
  /// Loudness in dB at the start.
  pub loudness_start: f32,
  /// Peak loudness in dB, `loudness_max_ms` after the start.
  pub loudness_max: f32,
  pub loudness_max_ms: u32,
  /// The strength of each pitch class, C to B, from 0 to 1.
  pub pitches: [f32; 12],
}

/// The audio analysis of the playing Spotify track, by base62 id.
#[derive(Debug, Default, PartialEq)]
pub enum AnalysisState {
  #[default]
  Idle,
  Loading(String),
  Ready(String, TrackAnalysis),
  /// Spotify has no analysis for the track.
  Unavailable(String),
  /// The fetch failed at this instant; it is tried again 10 s later.
  Failed(String, Instant),
}

/// What one fetch of an audio analysis found.
pub enum AnalysisFetch {
  Found(TrackAnalysis),
  Missing,
  Failed,
}

/// The wait before a failed fetch is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(10);

impl AnalysisState {
  fn track_id(&self) -> Option<&str> {
    match self {
      Self::Idle => None,
      Self::Loading(id) | Self::Ready(id, _) | Self::Unavailable(id) | Self::Failed(id, _) => {
        Some(id)
      }
    }
  }
}

impl App {
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn audio_analysis(&self) -> &AnalysisState {
    &self.audio_analysis
  }

  /// Fetches the analysis of `track_id` once, or again after a failure; `None` drops it.
  #[cfg_attr(not(test), allow(dead_code))]
  pub(crate) fn want_audio_analysis(&mut self, track_id: Option<&str>) {
    let Some(id) = track_id else {
      self.audio_analysis = AnalysisState::Idle;
      return;
    };
    let retry_due = matches!(
      self.audio_analysis,
      AnalysisState::Failed(_, at) if at.elapsed() >= RETRY_AFTER
    );
    if self.audio_analysis.track_id() == Some(id) && !retry_due {
      return;
    }
    self.audio_analysis = AnalysisState::Loading(id.to_string());
    self.dispatch_without_spinner(IoEvent::GetAudioAnalysis(id.to_string()));
  }

  pub(crate) fn audio_analysis_pending(&self, track_id: &str) -> bool {
    matches!(&self.audio_analysis, AnalysisState::Loading(id) if id == track_id)
  }

  /// Stores a fetch result, unless another track plays by now.
  pub(crate) fn set_audio_analysis(&mut self, track_id: &str, fetch: AnalysisFetch) {
    if !self.audio_analysis_pending(track_id) {
      return;
    }
    let id = track_id.to_string();
    self.audio_analysis = match fetch {
      AnalysisFetch::Found(analysis) => AnalysisState::Ready(id, analysis),
      AnalysisFetch::Missing => AnalysisState::Unavailable(id),
      AnalysisFetch::Failed => AnalysisState::Failed(id, Instant::now()),
    };
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::mpsc::{channel, Receiver};

  fn app_with_events() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    (App::new(tx, UserConfig::new(), None), rx)
  }

  #[test]
  fn wanting_a_track_fetches_its_analysis_once() {
    let (mut app, rx) = app_with_events();

    app.want_audio_analysis(Some("track1"));
    app.want_audio_analysis(Some("track1"));

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAudioAnalysis(id)) if id == "track1"));
    assert!(rx.try_recv().is_err());
    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Loading("track1".into())
    );
  }

  #[test]
  fn a_track_without_an_analysis_is_not_fetched_again() {
    let (mut app, rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));
    let _ = rx.try_recv();

    app.set_audio_analysis("track1", AnalysisFetch::Missing);
    app.want_audio_analysis(Some("track1"));

    assert!(rx.try_recv().is_err());
    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Unavailable("track1".into())
    );
  }

  #[test]
  fn a_failed_fetch_is_tried_again_only_after_the_wait() {
    let (mut app, rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));
    app.set_audio_analysis("track1", AnalysisFetch::Failed);
    let _ = rx.try_recv();

    app.want_audio_analysis(Some("track1"));
    assert!(rx.try_recv().is_err());

    app.audio_analysis = AnalysisState::Failed("track1".into(), Instant::now() - RETRY_AFTER);
    app.want_audio_analysis(Some("track1"));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAudioAnalysis(_))));
  }

  #[test]
  fn a_reply_for_a_track_no_longer_wanted_is_dropped() {
    let (mut app, _rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));
    app.want_audio_analysis(Some("track2"));

    app.set_audio_analysis("track1", AnalysisFetch::Found(TrackAnalysis::default()));

    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Loading("track2".into())
    );
  }

  #[test]
  fn a_reply_for_the_wanted_track_is_stored() {
    let (mut app, _rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));

    app.set_audio_analysis("track1", AnalysisFetch::Found(TrackAnalysis::default()));

    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Ready("track1".into(), TrackAnalysis::default())
    );
  }
}
