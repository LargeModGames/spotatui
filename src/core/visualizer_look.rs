//! The look of the GUI player-bar visualizer, read from `visualizer.yml`.
//!
//! Every key is optional. A mistyped value keeps its default and a number out of
//! range is clamped, each with a warning, so a typo never stops the app.

use crate::core::theme::{parse_theme_item, resolve, Color, Palette};
use serde::Serialize;
use serde_yaml::{Mapping, Value};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct VisualizerLook {
  /// Where the gradient's colours come from.
  pub source: LookSource,
  /// The colours of a `list` source.
  pub colors: Vec<[u8; 3]>,
  /// The colours when the cover gives none; empty means the accent.
  pub fallback: Vec<[u8; 3]>,
  /// How many colours to take from the cover, 1 to 8.
  pub count: u8,
  pub order: LookOrder,
  pub reverse: bool,
  /// Keep black, white and grey from a cover that also has colours.
  pub greys: bool,
  /// The OKLab distance two cover colours keep apart.
  pub min_distance: f64,
  /// The smallest part of the cover a colour may fill.
  pub min_share: f64,
  /// The least saturation of a coloured stop; greys stay grey.
  pub min_saturation: f64,
  /// The least and most luminance of a stop, in linear light.
  pub luminance_floor: f64,
  pub luminance_ceiling: f64,
  pub interpolation: LookInterpolation,
  /// How long the colours take to change to a new cover.
  pub fade_ms: u32,
  pub spectrum: SpectrumLook,
  pub waveform: WaveformLook,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LookSource {
  Cover,
  Accent,
  List,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LookOrder {
  Luminance,
  Hue,
  Share,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LookInterpolation {
  Srgb,
  Linear,
  Oklab,
}

/// Sizes in CSS pixels, alphas from 0 to 1, times in seconds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct SpectrumLook {
  pub gap: f64,
  pub cap_height: f64,
  pub cap_gap: f64,
  pub glow_side: f64,
  pub glow_end: f64,
  pub foot_alpha: f64,
  pub top_alpha: f64,
  pub glow_alpha: f64,
  pub glow_top_alpha: f64,
  pub cap_alpha: f64,
  pub pulse_alpha: f64,
  /// The bass pulse's height, as a share of the bar's height.
  pub pulse_height: f64,
  /// The share of the bands, from the bass up, whose level drives the pulse.
  pub pulse_bands: f64,
  /// Full heights a bar falls per second.
  pub fall_per_second: f64,
  pub peak_hold: f64,
  /// Heights per second squared.
  pub peak_gravity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct WaveformLook {
  /// How far the wave swings, as a share of the bar's height.
  pub reach: f64,
  pub fill_alpha: f64,
  /// CSS pixels per wave point, with at least min_points and at most 320.
  pub point_spacing: f64,
  pub min_points: f64,
  /// The layers of the line, drawn in order: mirror, width and alpha.
  pub lines: Vec<[f64; 3]>,
}

impl Default for VisualizerLook {
  fn default() -> Self {
    VisualizerLook {
      source: LookSource::Cover,
      colors: Vec::new(),
      fallback: Vec::new(),
      count: 5,
      order: LookOrder::Luminance,
      reverse: false,
      greys: false,
      min_distance: 0.08,
      min_share: 0.01,
      min_saturation: 0.0,
      luminance_floor: 0.22,
      luminance_ceiling: 0.35,
      interpolation: LookInterpolation::Srgb,
      fade_ms: 800,
      spectrum: SpectrumLook::default(),
      waveform: WaveformLook::default(),
    }
  }
}

impl Default for SpectrumLook {
  fn default() -> Self {
    SpectrumLook {
      gap: 2.0,
      cap_height: 2.0,
      cap_gap: 2.0,
      glow_side: 2.0,
      glow_end: 3.0,
      foot_alpha: 0.28,
      top_alpha: 0.08,
      glow_alpha: 0.12,
      glow_top_alpha: 0.036,
      cap_alpha: 0.7,
      pulse_alpha: 0.35,
      pulse_height: 0.5,
      pulse_bands: 0.125,
      fall_per_second: 3.0,
      peak_hold: 0.35,
      peak_gravity: 2.4,
    }
  }
}

impl Default for WaveformLook {
  fn default() -> Self {
    WaveformLook {
      reach: 0.46,
      fill_alpha: 0.14,
      point_spacing: 8.0,
      min_points: 75.0,
      lines: vec![
        [-0.6, 1.0, 0.18],
        [1.0, 9.0, 0.06],
        [1.0, 4.0, 0.16],
        [1.0, 1.6, 0.85],
      ],
    }
  }
}

const MAX_LINES: usize = 8;

/// What a read of the file found: its text, `None` while it is missing, or why it could not be read.
type FileRead = Result<Option<String>, String>;

/// Follows `visualizer.yml` across edits by its text, which is small enough to read on each check.
pub struct LookFile {
  /// The read last applied or reported.
  seen: FileRead,
  /// A failure seen once and not reported yet.
  pending: Option<String>,
}

impl Default for LookFile {
  fn default() -> Self {
    LookFile {
      seen: Ok(None),
      pending: None,
    }
  }
}

impl LookFile {
  /// The new look when the file changed; a failure counts only when the next check fails the same way.
  pub fn check(&mut self, path: &Path) -> Option<Result<(VisualizerLook, Vec<String>), String>> {
    let read: FileRead = match std::fs::read_to_string(path) {
      Ok(text) => Ok(Some(text)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    };
    if read == self.seen {
      self.pending = None;
      return None;
    }
    let loaded = match &read {
      Ok(Some(text)) => VisualizerLook::parse(text),
      Ok(None) => Ok((VisualizerLook::default(), Vec::new())),
      Err(error) => Err(error.clone()),
    };
    if let Err(error) = &loaded {
      // An editor's half-done save fails once; a broken file fails again.
      if self.pending.as_ref() != Some(error) {
        self.pending = Some(error.clone());
        return None;
      }
    }
    self.pending = None;
    self.seen = read;
    Some(loaded)
  }
}

impl VisualizerLook {
  /// Fails only when the text is not YAML; each bad value is a warning.
  pub fn parse(text: &str) -> Result<(Self, Vec<String>), String> {
    let value: Value = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    let look = Self::from_value(&value, &mut warnings);
    Ok((look, warnings))
  }

  fn from_value(value: &Value, warnings: &mut Vec<String>) -> Self {
    let d = Self::default();
    let mut f = Fields::new(Some(value), "", warnings);
    let (source, colors) = match f.get("colors") {
      None => (d.source, d.colors),
      Some(Value::String(word)) if word == "cover" => (LookSource::Cover, Vec::new()),
      Some(Value::String(word)) if word == "accent" => (LookSource::Accent, Vec::new()),
      Some(Value::Sequence(items)) => match color_list(items, "colors", warnings) {
        list if list.is_empty() => {
          warnings.push("colors: no valid colour in the list, used cover".to_string());
          (d.source, d.colors)
        }
        list => (LookSource::List, list),
      },
      Some(other) => {
        warnings.push(format!(
          "colors: expected cover, accent or a list of colours, not {}",
          shown(other)
        ));
        (d.source, d.colors)
      }
    };
    let fallback = match f.get("fallback") {
      None => d.fallback,
      Some(Value::String(word)) if word == "accent" => Vec::new(),
      Some(Value::Sequence(items)) => color_list(items, "fallback", warnings),
      Some(other) => {
        warnings.push(format!(
          "fallback: expected accent or a list of colours, not {}",
          shown(other)
        ));
        d.fallback
      }
    };
    let count = f
      .number("count", 1.0, 8.0, d.count.into(), warnings)
      .round() as u8;
    let order = f.choice(
      "order",
      &[
        ("luminance", LookOrder::Luminance),
        ("hue", LookOrder::Hue),
        ("share", LookOrder::Share),
      ],
      d.order,
      warnings,
    );
    let reverse = f.flag("reverse", d.reverse, warnings);
    let greys = f.flag("greys", d.greys, warnings);
    let min_distance = f.number("min_distance", 0.0, 0.5, d.min_distance, warnings);
    let min_share = f.number("min_share", 0.0, 0.5, d.min_share, warnings);
    let min_saturation = f.number("min_saturation", 0.0, 1.0, d.min_saturation, warnings);
    let luminance_ceiling = f.number("luminance_ceiling", 0.0, 1.0, d.luminance_ceiling, warnings);
    let mut luminance_floor = f.number("luminance_floor", 0.0, 1.0, d.luminance_floor, warnings);
    if luminance_floor > luminance_ceiling {
      warnings.push(format!(
        "luminance_floor: {luminance_floor} is above luminance_ceiling, used {luminance_ceiling}"
      ));
      luminance_floor = luminance_ceiling;
    }
    let interpolation = f.choice(
      "interpolation",
      &[
        ("srgb", LookInterpolation::Srgb),
        ("linear", LookInterpolation::Linear),
        ("oklab", LookInterpolation::Oklab),
      ],
      d.interpolation,
      warnings,
    );
    let fade_ms = f
      .number("fade_ms", 0.0, 5000.0, d.fade_ms.into(), warnings)
      .round() as u32;
    let spectrum = SpectrumLook::from_value(f.get("spectrum"), warnings);
    let waveform = WaveformLook::from_value(f.get("waveform"), warnings);
    f.finish(warnings);
    VisualizerLook {
      source,
      colors,
      fallback,
      count,
      order,
      reverse,
      greys,
      min_distance,
      min_share,
      min_saturation,
      luminance_floor,
      luminance_ceiling,
      interpolation,
      fade_ms,
      spectrum,
      waveform,
    }
  }
}

impl SpectrumLook {
  fn from_value(value: Option<&Value>, warnings: &mut Vec<String>) -> Self {
    let d = Self::default();
    let mut f = Fields::new(value, "spectrum.", warnings);
    let look = SpectrumLook {
      gap: f.number("gap", 0.0, 20.0, d.gap, warnings),
      cap_height: f.number("cap_height", 0.0, 20.0, d.cap_height, warnings),
      cap_gap: f.number("cap_gap", 0.0, 20.0, d.cap_gap, warnings),
      glow_side: f.number("glow_side", 0.0, 20.0, d.glow_side, warnings),
      glow_end: f.number("glow_end", 0.0, 20.0, d.glow_end, warnings),
      foot_alpha: f.number("foot_alpha", 0.0, 1.0, d.foot_alpha, warnings),
      top_alpha: f.number("top_alpha", 0.0, 1.0, d.top_alpha, warnings),
      glow_alpha: f.number("glow_alpha", 0.0, 1.0, d.glow_alpha, warnings),
      glow_top_alpha: f.number("glow_top_alpha", 0.0, 1.0, d.glow_top_alpha, warnings),
      cap_alpha: f.number("cap_alpha", 0.0, 1.0, d.cap_alpha, warnings),
      pulse_alpha: f.number("pulse_alpha", 0.0, 1.0, d.pulse_alpha, warnings),
      pulse_height: f.number("pulse_height", 0.0, 1.0, d.pulse_height, warnings),
      pulse_bands: f.number("pulse_bands", 0.01, 1.0, d.pulse_bands, warnings),
      fall_per_second: f.number("fall_per_second", 0.1, 60.0, d.fall_per_second, warnings),
      peak_hold: f.number("peak_hold", 0.0, 10.0, d.peak_hold, warnings),
      peak_gravity: f.number("peak_gravity", 0.0, 100.0, d.peak_gravity, warnings),
    };
    f.finish(warnings);
    look
  }
}

impl WaveformLook {
  fn from_value(value: Option<&Value>, warnings: &mut Vec<String>) -> Self {
    let d = Self::default();
    let mut f = Fields::new(value, "waveform.", warnings);
    let reach = f.number("reach", 0.0, 0.5, d.reach, warnings);
    let fill_alpha = f.number("fill_alpha", 0.0, 1.0, d.fill_alpha, warnings);
    let point_spacing = f.number("point_spacing", 1.0, 64.0, d.point_spacing, warnings);
    let min_points = f
      .number("min_points", 2.0, 320.0, d.min_points, warnings)
      .round();
    let lines = match f.get("lines") {
      None => d.lines,
      Some(Value::Sequence(items)) => {
        if items.len() > MAX_LINES {
          warnings.push(format!(
            "waveform.lines: {} lines, used the first {MAX_LINES}",
            items.len()
          ));
        }
        let lines: Vec<[f64; 3]> = items
          .iter()
          .take(MAX_LINES)
          .enumerate()
          .filter_map(|(index, item)| wave_line(item, index, warnings))
          .collect();
        if lines.is_empty() && !items.is_empty() {
          warnings
            .push("waveform.lines: no valid line in the list, used the default lines".to_string());
          d.lines
        } else {
          lines
        }
      }
      Some(other) => {
        warnings.push(format!(
          "waveform.lines: expected a list of [mirror, width, alpha], not {}",
          shown(other)
        ));
        d.lines
      }
    };
    f.finish(warnings);
    WaveformLook {
      reach,
      fill_alpha,
      point_spacing,
      min_points,
      lines,
    }
  }
}

fn wave_line(item: &Value, index: usize, warnings: &mut Vec<String>) -> Option<[f64; 3]> {
  let parts: Option<Vec<f64>> = item
    .as_sequence()
    .filter(|parts| parts.len() == 3)
    .and_then(|parts| parts.iter().map(Value::as_f64).collect());
  let Some(parts) = parts.filter(|parts| parts.iter().all(|part| part.is_finite())) else {
    warnings.push(format!(
      "waveform.lines[{index}]: expected [mirror, width, alpha], left it out"
    ));
    return None;
  };
  let line = [
    parts[0].clamp(-2.0, 2.0),
    parts[1].clamp(0.0, 40.0),
    parts[2].clamp(0.0, 1.0),
  ];
  if line[..] != parts[..] {
    warnings.push(format!(
      "waveform.lines[{index}]: {parts:?} is outside mirror -2 to 2, width 0 to 40, alpha 0 to 1; used {line:?}"
    ));
  }
  Some(line)
}

fn color_list(items: &[Value], key: &str, warnings: &mut Vec<String>) -> Vec<[u8; 3]> {
  items
    .iter()
    .enumerate()
    .filter_map(|(index, item)| {
      let parsed = color(item);
      if parsed.is_none() {
        warnings.push(format!(
          "{key}[{index}]: {} is not a colour, left it out",
          shown(item)
        ));
      }
      parsed
    })
    .collect()
}

/// `#rgb`, `#rrggbb`, `"r, g, b"` or a theme colour name.
fn color(value: &Value) -> Option<[u8; 3]> {
  let text = value.as_str()?.trim();
  if let Some(hex) = text.strip_prefix('#') {
    return hex_color(hex);
  }
  match parse_theme_item(text) {
    Ok(Color::Reset) | Err(_) => None,
    Ok(color) => Some(resolve(color, &Palette::default())),
  }
}

fn hex_color(hex: &str) -> Option<[u8; 3]> {
  if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
    return None;
  }
  let channel = |digits: &str| u8::from_str_radix(digits, 16).ok();
  match hex.len() {
    3 => {
      let mut rgb = [0; 3];
      for (slot, digit) in rgb.iter_mut().zip(hex.chars()) {
        *slot = channel(&digit.to_string())? * 17;
      }
      Some(rgb)
    }
    6 => Some([
      channel(&hex[0..2])?,
      channel(&hex[2..4])?,
      channel(&hex[4..6])?,
    ]),
    _ => None,
  }
}

fn shown(value: &Value) -> String {
  serde_yaml::to_string(value)
    .map(|text| text.trim().to_string())
    .unwrap_or_default()
}

/// One map of the file, which remembers the keys it was asked for.
struct Fields<'a> {
  map: Option<&'a Mapping>,
  path: &'static str,
  seen: Vec<&'static str>,
}

impl<'a> Fields<'a> {
  fn new(value: Option<&'a Value>, path: &'static str, warnings: &mut Vec<String>) -> Self {
    let map = match value {
      None | Some(Value::Null) => None,
      Some(Value::Mapping(map)) => Some(map),
      Some(other) => {
        let name = if path.is_empty() {
          "visualizer.yml".to_string()
        } else {
          path.trim_end_matches('.').to_string()
        };
        warnings.push(format!(
          "{name}: expected a map of settings, not {}; kept the defaults",
          shown(other)
        ));
        None
      }
    };
    Fields {
      map,
      path,
      seen: Vec::new(),
    }
  }

  fn get(&mut self, key: &'static str) -> Option<&'a Value> {
    self.seen.push(key);
    self.map?.get(key).filter(|value| !value.is_null())
  }

  fn number(
    &mut self,
    key: &'static str,
    min: f64,
    max: f64,
    default: f64,
    warnings: &mut Vec<String>,
  ) -> f64 {
    let Some(value) = self.get(key) else {
      return default;
    };
    match value.as_f64().filter(|number| number.is_finite()) {
      None => {
        warnings.push(format!(
          "{}{key}: expected a number, not {}; kept {default}",
          self.path,
          shown(value)
        ));
        default
      }
      Some(number) if number < min || number > max => {
        let used = number.clamp(min, max);
        warnings.push(format!(
          "{}{key}: {number} is outside {min} to {max}, used {used}",
          self.path
        ));
        used
      }
      Some(number) => number,
    }
  }

  fn flag(&mut self, key: &'static str, default: bool, warnings: &mut Vec<String>) -> bool {
    let Some(value) = self.get(key) else {
      return default;
    };
    value.as_bool().unwrap_or_else(|| {
      warnings.push(format!(
        "{}{key}: expected true or false, not {}; kept {default}",
        self.path,
        shown(value)
      ));
      default
    })
  }

  fn choice<T: Copy>(
    &mut self,
    key: &'static str,
    options: &[(&str, T)],
    default: T,
    warnings: &mut Vec<String>,
  ) -> T {
    let Some(value) = self.get(key) else {
      return default;
    };
    let found = value
      .as_str()
      .and_then(|word| options.iter().find(|(name, _)| *name == word));
    match found {
      Some((_, option)) => *option,
      None => {
        let names: Vec<&str> = options.iter().map(|(name, _)| *name).collect();
        warnings.push(format!(
          "{}{key}: expected {}, not {}",
          self.path,
          names.join(", "),
          shown(value)
        ));
        default
      }
    }
  }

  fn finish(self, warnings: &mut Vec<String>) {
    let Some(map) = self.map else {
      return;
    };
    for key in map.keys() {
      let known = key.as_str().is_some_and(|key| self.seen.contains(&key));
      if !known {
        warnings.push(format!("{}{}: unknown setting", self.path, shown(key)));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parsed(text: &str) -> (VisualizerLook, Vec<String>) {
    VisualizerLook::parse(text).unwrap()
  }

  #[test]
  fn an_empty_file_is_the_default_look() {
    assert_eq!(parsed(""), (VisualizerLook::default(), Vec::new()));
  }

  #[test]
  fn text_that_is_not_yaml_is_an_error() {
    assert!(VisualizerLook::parse("colors: [unclosed").is_err());
  }

  #[test]
  fn a_wrong_type_keeps_only_that_keys_default() {
    let (look, warnings) = parsed("count: many\nfade_ms: 200\n");
    assert_eq!(look.count, 5);
    assert_eq!(look.fade_ms, 200);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].starts_with("count:"), "{warnings:?}");
  }

  #[test]
  fn numbers_outside_their_range_are_clamped_with_a_warning() {
    let (look, warnings) = parsed("count: 40\nspectrum:\n  gap: -3\n");
    assert_eq!(look.count, 8);
    assert_eq!(look.spectrum.gap, 0.0);
    assert_eq!(warnings.len(), 2);
    assert!(warnings[1].starts_with("spectrum.gap:"), "{warnings:?}");
  }

  #[test]
  fn a_number_that_is_not_finite_keeps_the_default() {
    let (look, warnings) = parsed("fade_ms: .nan\n");
    assert_eq!(look.fade_ms, 800);
    assert_eq!(warnings.len(), 1);
  }

  #[test]
  fn a_floor_above_the_ceiling_is_pulled_down_to_it() {
    let (look, warnings) = parsed("luminance_floor: 0.6\nluminance_ceiling: 0.4\n");
    assert_eq!(look.luminance_floor, 0.4);
    assert_eq!(warnings.len(), 1);
  }

  #[test]
  fn an_unknown_key_warns_at_its_path() {
    let (_, warnings) = parsed("colours: cover\nwaveform:\n  glow: 3\n");
    assert_eq!(
      warnings,
      vec![
        "waveform.glow: unknown setting".to_string(),
        "colours: unknown setting".to_string(),
      ]
    );
  }

  #[test]
  fn an_unknown_word_keeps_the_default_choice() {
    let (look, warnings) = parsed("order: rainbow\ninterpolation: oklab\n");
    assert_eq!(look.order, LookOrder::Luminance);
    assert_eq!(look.interpolation, LookInterpolation::Oklab);
    assert_eq!(
      warnings,
      vec!["order: expected luminance, hue, share, not rainbow"]
    );
  }

  #[test]
  fn colours_take_hex_short_hex_rgb_and_theme_names() {
    let (look, warnings) =
      parsed("colors: ['#1e1e2e', '#fff', '243, 139, 168', LightBlue, Reset, '#12345']\n");
    assert_eq!(look.source, LookSource::List);
    assert_eq!(
      look.colors,
      vec![
        [0x1e, 0x1e, 0x2e],
        [255, 255, 255],
        [243, 139, 168],
        resolve(Color::LightBlue, &Palette::default()),
      ]
    );
    assert_eq!(warnings.len(), 2, "{warnings:?}");
  }

  #[test]
  fn a_list_with_no_valid_colour_falls_back_to_the_cover() {
    let (look, warnings) = parsed("colors: [nope]\n");
    assert_eq!(look.source, LookSource::Cover);
    assert!(look.colors.is_empty());
    assert_eq!(warnings.len(), 2);
  }

  #[test]
  fn accent_is_a_source_and_a_fallback() {
    let (look, warnings) = parsed("colors: accent\nfallback: accent\n");
    assert_eq!(look.source, LookSource::Accent);
    assert!(look.fallback.is_empty());
    assert!(warnings.is_empty());
    let (look, _) = parsed("fallback: ['#000000', White]\n");
    assert_eq!(look.fallback, vec![[0, 0, 0], [255, 255, 255]]);
  }

  #[test]
  fn a_section_that_is_not_a_map_keeps_its_defaults() {
    let (look, warnings) = parsed("spectrum: 3\n");
    assert_eq!(look.spectrum, SpectrumLook::default());
    assert_eq!(warnings.len(), 1);
    let (look, warnings) = parsed("- cover\n");
    assert_eq!(look, VisualizerLook::default());
    assert_eq!(warnings.len(), 1);
  }

  #[test]
  fn wave_lines_drop_bad_entries_and_clamp_the_rest() {
    let (look, warnings) =
      parsed("waveform:\n  lines: [[1, 50, 2], [1, 2], oops, [0.5, 3, 0.4]]\n");
    assert_eq!(look.waveform.lines, vec![[1.0, 40.0, 1.0], [0.5, 3.0, 0.4]]);
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    assert!(
      warnings[0].starts_with("waveform.lines[0]:"),
      "{warnings:?}"
    );
  }

  #[test]
  fn a_line_list_with_nothing_valid_keeps_the_default_lines_and_an_empty_one_draws_none() {
    let (look, warnings) = parsed("waveform:\n  lines: [oops, [1, 2]]\n");
    assert_eq!(look.waveform.lines, WaveformLook::default().lines);
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    let (look, warnings) = parsed("waveform:\n  lines: []\n");
    assert!(look.waveform.lines.is_empty());
    assert!(warnings.is_empty());
  }

  #[test]
  fn a_full_file_reads_every_key() {
    let text = "\
colors: cover
fallback: accent
count: 3
order: hue
reverse: true
greys: true
min_distance: 0.1
min_share: 0.02
min_saturation: 0.3
luminance_floor: 0.05
luminance_ceiling: 0.5
interpolation: linear
fade_ms: 0
spectrum:
  gap: 1
  cap_height: 0
  cap_gap: 1
  glow_side: 0
  glow_end: 0
  foot_alpha: 0.5
  top_alpha: 0.5
  glow_alpha: 0
  glow_top_alpha: 0
  cap_alpha: 1
  pulse_alpha: 0
  pulse_height: 1
  pulse_bands: 0.5
  fall_per_second: 6
  peak_hold: 1
  peak_gravity: 5
waveform:
  reach: 0.3
  fill_alpha: 0
  point_spacing: 4
  min_points: 2
  lines: [[1, 2, 1]]
";
    let (look, warnings) = parsed(text);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(look.count, 3);
    assert_eq!(look.order, LookOrder::Hue);
    assert!(look.reverse);
    assert!(look.greys);
    assert_eq!(look.interpolation, LookInterpolation::Linear);
    assert_eq!(look.fade_ms, 0);
    assert_eq!(look.spectrum.cap_height, 0.0);
    assert_eq!(look.spectrum.fall_per_second, 6.0);
    assert_eq!(look.waveform.point_spacing, 4.0);
    assert_eq!(look.waveform.min_points, 2.0);
    assert_eq!(look.spectrum.pulse_bands, 0.5);
    assert_eq!(look.waveform.lines, vec![[1.0, 2.0, 1.0]]);
  }

  fn count_of(checked: Option<Result<(VisualizerLook, Vec<String>), String>>) -> Option<u8> {
    checked.map(|loaded| loaded.unwrap().0.count)
  }

  #[test]
  fn each_edit_is_taken_once_and_a_missing_file_is_the_default_look() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("visualizer.yml");
    let mut file = LookFile::default();
    assert!(file.check(&path).is_none());
    std::fs::write(&path, "count: 2\n").unwrap();
    assert_eq!(count_of(file.check(&path)), Some(2));
    assert!(file.check(&path).is_none());
    // The same size at once, as a script or a coarse clock can leave it.
    std::fs::write(&path, "count: 3\n").unwrap();
    assert_eq!(count_of(file.check(&path)), Some(3));
    std::fs::remove_file(&path).unwrap();
    assert_eq!(count_of(file.check(&path)), Some(5));
  }

  #[test]
  fn a_broken_file_is_reported_on_its_second_failed_read_and_then_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("visualizer.yml");
    let mut file = LookFile::default();
    std::fs::write(&path, "colors: [unclosed\n").unwrap();
    assert!(file.check(&path).is_none());
    assert!(matches!(file.check(&path), Some(Err(_))));
    assert!(file.check(&path).is_none());
  }

  #[test]
  fn a_half_saved_file_that_is_finished_by_the_next_read_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("visualizer.yml");
    let mut file = LookFile::default();
    std::fs::write(&path, "colors: [unclosed\n").unwrap();
    assert!(file.check(&path).is_none());
    std::fs::write(&path, "colors: ['#ff0000']\n").unwrap();
    let (look, problems) = file.check(&path).unwrap().unwrap();
    assert_eq!(look.colors, vec![[255, 0, 0]]);
    assert!(problems.is_empty());
  }
}
