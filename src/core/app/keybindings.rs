use super::*;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CapabilityState {
  #[default]
  Unknown,
  Yes,
  No,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalInputCapabilities {
  pub keyboard_enhancement_supported: bool,
  pub keyboard_enhancement_enabled: bool,
  pub ctrl_punct_reliable: CapabilityState,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyFallbackReason {
  CtrlCommaNotReported,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeybindingRuntimeState {
  pub effective_open_settings: Option<Key>,
  pub fallback_reason: Option<KeyFallbackReason>,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  pub fallback_notice_shown: bool,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  pub persist_prompt_shown: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PendingKeybindingPersist {
  pub open_settings_key: Key,
}

impl App {
  pub fn effective_open_settings_key(&self) -> Key {
    self
      .keybinding_runtime
      .effective_open_settings
      .unwrap_or(self.user_config.keys.open_settings)
  }

  pub fn effective_save_settings_key(&self) -> Key {
    self.user_config.keys.save_settings
  }

  #[cfg(target_os = "macos")]
  fn allow_plain_comma_open_settings_fallback(&self) -> bool {
    !matches!(
      self.get_current_route().active_block,
      ActiveBlock::Input
        | ActiveBlock::TrackTable
        | ActiveBlock::AlbumList
        | ActiveBlock::Artists
        | ActiveBlock::SortMenu
        | ActiveBlock::Settings
        | ActiveBlock::Dialog(_)
    )
  }

  #[cfg(target_os = "macos")]
  pub fn maybe_activate_open_settings_fallback(&mut self, key: Key) -> bool {
    if self.user_config.keys.open_settings != Key::Ctrl(',') {
      return false;
    }

    if key == Key::Ctrl(',') {
      self.view.terminal_input_caps.ctrl_punct_reliable = CapabilityState::Yes;
      self.keybinding_runtime.effective_open_settings = None;
      self.keybinding_runtime.fallback_reason = None;
      return false;
    }

    if key == Key::Char(',') && self.allow_plain_comma_open_settings_fallback() {
      self.view.terminal_input_caps.ctrl_punct_reliable = CapabilityState::No;
      self.keybinding_runtime.effective_open_settings = Some(Key::Alt(','));
      self.keybinding_runtime.fallback_reason = Some(KeyFallbackReason::CtrlCommaNotReported);

      if !self.keybinding_runtime.fallback_notice_shown {
        self.set_status_message(
          "Ctrl+, not detected in this terminal; using Alt+, for this session",
          5,
        );
        self.keybinding_runtime.fallback_notice_shown = true;
      }

      if !self.keybinding_runtime.persist_prompt_shown {
        self.keybinding_runtime.persist_prompt_shown = true;
        self.pending_keybinding_persist = Some(PendingKeybindingPersist {
          open_settings_key: Key::Alt(','),
        });
        self.view.confirm = false;
      }

      return true;
    }

    false
  }

  #[cfg(not(target_os = "macos"))]
  pub fn maybe_activate_open_settings_fallback(&mut self, _key: Key) -> bool {
    false
  }

  pub fn persist_open_settings_fallback(&mut self) {
    let Some(persist) = self.pending_keybinding_persist else {
      return;
    };

    self.user_config.keys.open_settings = persist.open_settings_key;
    if let Err(e) = self.user_config.save_config() {
      self.handle_error(anyhow!("Failed to save keybinding fallback: {}", e));
      return;
    }

    self.keybinding_runtime.effective_open_settings = None;
    self.keybinding_runtime.fallback_reason = None;
    self.set_status_message(
      format!(
        "Saved open settings shortcut as {}",
        persist.open_settings_key
      ),
      4,
    );
  }
}
