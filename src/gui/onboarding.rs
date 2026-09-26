//! The first-launch questions answered from the page: boot asks through
//! [`Onboarding`] before any `App` exists, and each question waits for the page.

use crate::core::onboarding::{Onboarding, OnboardingAnswer, OnboardingPrompt};
use crate::core::source::Source;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// How long a question waits with no page connected: the launch code's lifetime.
pub(crate) const NO_PAGE_GRACE: Duration = Duration::from_secs(60);

/// Everything the page shows before boot: what was said so far, and the open question.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct OnboardingView {
  pub(crate) transcript: String,
  pub(crate) pending: Option<OnboardingQuestion>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct OnboardingQuestion {
  pub(crate) seq: u64,
  pub(crate) ask: OnboardingAsk,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "kind")]
pub(crate) enum OnboardingAsk {
  Line {
    prompt: String,
    masked: bool,
  },
  Confirm {
    title: String,
    body: String,
    question: String,
  },
  PickSources {
    options: Vec<SourceChoice>,
  },
}

/// One source the first-run picker offers, with the text the terminal picker shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SourceChoice {
  pub(crate) source: Source,
  pub(crate) label: String,
  pub(crate) note: String,
}

/// The page's answer to the question with the same `seq`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct OnboardingReply {
  pub(crate) seq: u64,
  pub(crate) answer: OnboardingReplyAnswer,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "kind")]
pub(crate) enum OnboardingReplyAnswer {
  Line { text: String },
  Confirm { yes: bool },
  Sources { picked: Vec<Source> },
}

pub(crate) struct BrowserOnboarding {
  view: watch::Sender<OnboardingView>,
  replies_tx: mpsc::Sender<OnboardingReply>,
  replies: Mutex<mpsc::Receiver<OnboardingReply>>,
  next_seq: AtomicU64,
  pages: AtomicUsize,
  no_page_grace: Duration,
}

/// Counts one connected page for as long as it lives.
pub(crate) struct PageConnection<'a>(&'a AtomicUsize);

impl Drop for PageConnection<'_> {
  fn drop(&mut self) {
    self.0.fetch_sub(1, Ordering::Relaxed);
  }
}

impl BrowserOnboarding {
  pub(crate) fn new() -> Self {
    let (replies_tx, replies) = mpsc::channel();
    BrowserOnboarding {
      view: watch::Sender::new(OnboardingView::default()),
      replies_tx,
      replies: Mutex::new(replies),
      next_seq: AtomicU64::new(0),
      pages: AtomicUsize::new(0),
      no_page_grace: NO_PAGE_GRACE,
    }
  }

  pub(crate) fn subscribe(&self) -> watch::Receiver<OnboardingView> {
    self.view.subscribe()
  }

  /// Passes on a reply to the open question; any other reply is dropped.
  pub(crate) fn answer(&self, reply: OnboardingReply) {
    let open = self
      .view
      .borrow()
      .pending
      .as_ref()
      .map(|question| question.seq);
    if open == Some(reply.seq) {
      let _ = self.replies_tx.send(reply);
    }
  }

  /// Waits until a page is connected, up to `limit`.
  pub(crate) async fn wait_for_page(&self, limit: Duration) {
    let deadline = Instant::now() + limit;
    while self.pages.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
      tokio::time::sleep(Duration::from_millis(100)).await;
    }
  }

  pub(crate) fn page_connected(&self) -> PageConnection<'_> {
    self.pages.fetch_add(1, Ordering::Relaxed);
    PageConnection(&self.pages)
  }

  /// Shows `ask` and blocks until the page answers it; never call it on a runtime worker.
  fn ask_page(&self, ask: OnboardingAsk) -> Result<OnboardingReplyAnswer> {
    let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
    self
      .view
      .send_modify(|view| view.pending = Some(OnboardingQuestion { seq, ask }));
    let replies = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
    let mut alone_since = None;
    let answer = loop {
      match replies.recv_timeout(Duration::from_secs(1)) {
        Ok(reply) if reply.seq == seq => break Ok(reply.answer),
        Ok(_stale) => {}
        Err(_) if self.pages.load(Ordering::Relaxed) > 0 => alone_since = None,
        Err(_) => {
          if alone_since.get_or_insert_with(Instant::now).elapsed() >= self.no_page_grace {
            break Err(anyhow!("no browser page is connected"));
          }
        }
      }
    };
    self.view.send_modify(|view| view.pending = None);
    answer
  }

  /// A typed line comes back the way `read_line` returns it: one line, newline included.
  fn line(&self, prompt: &str, masked: bool) -> Result<String> {
    match self.ask_page(OnboardingAsk::Line {
      prompt: prompt.to_string(),
      masked,
    })? {
      OnboardingReplyAnswer::Line { text } => {
        let line = text.split(['\r', '\n']).next().unwrap_or_default();
        Ok(format!("{line}\n"))
      }
      _ => Err(anyhow!("the page answered a line prompt with another kind")),
    }
  }
}

impl Onboarding for BrowserOnboarding {
  fn info(&self, text: &str) {
    self.view.send_modify(|view| {
      view.transcript.push_str(text);
      view.transcript.push('\n');
    });
  }

  fn progress(&self, text: &str) {
    self.view.send_modify(|view| view.transcript.push_str(text));
  }

  fn prompt_line(&self, prompt: &str) -> Result<String> {
    self.line(prompt, false)
  }

  fn prompt_masked(&self, prompt: &str) -> Result<String> {
    self.line(prompt, true)
  }

  fn is_interactive(&self) -> bool {
    true
  }

  fn ask(&self, prompt: &OnboardingPrompt) -> Result<OnboardingAnswer> {
    let OnboardingPrompt::Confirm {
      title,
      body,
      question,
    } = prompt.clone();
    match self.ask_page(OnboardingAsk::Confirm {
      title,
      body,
      question,
    })? {
      OnboardingReplyAnswer::Confirm { yes: true } => Ok(OnboardingAnswer::Yes),
      OnboardingReplyAnswer::Confirm { yes: false } => Ok(OnboardingAnswer::No),
      _ => Err(anyhow!(
        "the page answered a yes/no question with another kind"
      )),
    }
  }

  fn pick_sources(&self, options: &[Source]) -> Result<Option<Vec<Source>>> {
    let choices = options
      .iter()
      .map(|source| SourceChoice {
        source: *source,
        label: source.label().to_string(),
        note: source.note().to_string(),
      })
      .collect();
    match self.ask_page(OnboardingAsk::PickSources { options: choices })? {
      OnboardingReplyAnswer::Sources { picked } => {
        // Offered order: the first pick becomes the active source.
        let picked: Vec<Source> = options
          .iter()
          .copied()
          .filter(|option| picked.contains(option))
          .collect();
        Ok((!picked.is_empty()).then_some(picked))
      }
      _ => Err(anyhow!(
        "the page answered the source picker with another kind"
      )),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Arc;

  /// Answers the next question the bridge opens, and returns what it asked.
  fn answer_next(
    bridge: &Arc<BrowserOnboarding>,
    answer: OnboardingReplyAnswer,
  ) -> std::thread::JoinHandle<OnboardingAsk> {
    let bridge = Arc::clone(bridge);
    let mut view = bridge.subscribe();
    std::thread::spawn(move || {
      let question = futures::executor::block_on(view.wait_for(|view| view.pending.is_some()))
        .unwrap()
        .pending
        .clone()
        .unwrap();
      bridge.answer(OnboardingReply {
        seq: question.seq,
        answer,
      });
      question.ask
    })
  }

  fn line(text: &str) -> OnboardingReplyAnswer {
    OnboardingReplyAnswer::Line {
      text: text.to_string(),
    }
  }

  #[test]
  fn a_line_typed_on_the_page_comes_back_the_way_read_line_returns_it() {
    let bridge = Arc::new(BrowserOnboarding::new());
    let page = answer_next(&bridge, line("https://127.0.0.1:8888/callback?code=x"));

    let answer = bridge
      .prompt_line("Enter the URL you were redirected to: ")
      .unwrap();

    assert_eq!(answer, "https://127.0.0.1:8888/callback?code=x\n");
    assert_eq!(
      page.join().unwrap(),
      OnboardingAsk::Line {
        prompt: "Enter the URL you were redirected to: ".to_string(),
        masked: false
      }
    );
    assert_eq!(bridge.subscribe().borrow().pending, None);
  }

  #[test]
  fn a_masked_prompt_reaches_the_page_marked_masked() {
    let bridge = Arc::new(BrowserOnboarding::new());
    let page = answer_next(&bridge, line("hunter2"));

    assert_eq!(bridge.prompt_masked("  Password: ").unwrap(), "hunter2\n");
    assert_eq!(
      page.join().unwrap(),
      OnboardingAsk::Line {
        prompt: "  Password: ".to_string(),
        masked: true
      }
    );
  }

  #[test]
  fn a_no_on_the_page_answers_the_confirm_prompt_no() {
    let bridge = Arc::new(BrowserOnboarding::new());
    let page = answer_next(&bridge, OnboardingReplyAnswer::Confirm { yes: false });

    let answer = bridge
      .ask(&OnboardingPrompt::Confirm {
        title: "T".to_string(),
        body: "B".to_string(),
        question: "Q?".to_string(),
      })
      .unwrap();

    assert_eq!(answer, OnboardingAnswer::No);
    assert_eq!(
      page.join().unwrap(),
      OnboardingAsk::Confirm {
        title: "T".to_string(),
        body: "B".to_string(),
        question: "Q?".to_string()
      }
    );
  }

  #[test]
  fn a_source_pick_keeps_only_offered_sources_in_offered_order() {
    let bridge = Arc::new(BrowserOnboarding::new());
    let options = [Source::Spotify, Source::Local];
    let page = answer_next(
      &bridge,
      OnboardingReplyAnswer::Sources {
        picked: vec![Source::Local, Source::Spotify, Source::Qobuz],
      },
    );

    let picked = bridge.pick_sources(&options).unwrap();

    assert_eq!(picked, Some(vec![Source::Spotify, Source::Local]));
    assert_eq!(
      page.join().unwrap(),
      OnboardingAsk::PickSources {
        options: options
          .iter()
          .map(|source| SourceChoice {
            source: *source,
            label: source.label().to_string(),
            note: source.note().to_string(),
          })
          .collect()
      }
    );
    let page = answer_next(&bridge, OnboardingReplyAnswer::Sources { picked: vec![] });
    assert_eq!(bridge.pick_sources(&options).unwrap(), None);
    page.join().unwrap();
  }

  #[test]
  fn a_late_reply_to_an_answered_question_does_not_answer_the_next_one() {
    let bridge = Arc::new(BrowserOnboarding::new());
    let page = answer_next(&bridge, line("1"));
    assert_eq!(bridge.prompt_line("first").unwrap(), "1\n");
    page.join().unwrap();

    bridge.answer(OnboardingReply {
      seq: 0,
      answer: line("stale"),
    });
    let page = answer_next(&bridge, line("2"));

    assert_eq!(bridge.prompt_line("second").unwrap(), "2\n");
    page.join().unwrap();
  }

  #[test]
  fn a_page_that_connects_later_sees_everything_said_so_far() {
    let bridge = BrowserOnboarding::new();
    bridge.progress("Testing connection... ");
    bridge.info("OK");

    let view = bridge.subscribe().borrow().clone();

    assert_eq!(view.transcript, "Testing connection... OK\n");
    assert_eq!(view.pending, None);
  }

  #[test]
  fn a_mismatched_answer_kind_is_an_error_not_a_guess() {
    let bridge = Arc::new(BrowserOnboarding::new());
    let page = answer_next(&bridge, OnboardingReplyAnswer::Confirm { yes: true });

    assert!(bridge.prompt_line("x").is_err());
    page.join().unwrap();
  }

  #[test]
  fn a_question_gives_up_when_no_page_is_connected() {
    let mut bridge = BrowserOnboarding::new();
    bridge.no_page_grace = Duration::ZERO;

    let error = bridge.prompt_line("x").unwrap_err();

    assert_eq!(error.to_string(), "no browser page is connected");
  }
}
