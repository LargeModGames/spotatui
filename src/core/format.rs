//! Format-string templates for customizable playbar / window-title text.
//!
//! A [`Template`] is a sequence of literal segments and `{placeholder}` segments.
//! The syntax deliberately mirrors `format!` placeholders loosely:
//! - `{key}` is substituted with the named value at render time.
//! - `{{` and `}}` produce a literal `{` and `}`.
//!
//! Width specifiers are **not** supported in v1; callers that need a padded
//! field (e.g. the 7-char play state) pre-pad the value before substituting.

use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// One piece of a parsed [`Template`].
#[derive(Clone, Debug, PartialEq)]
enum Segment {
  Literal(String),
  Placeholder(usize),
}

/// A parsed, reusable format template.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
  segments: Vec<Segment>,
}

impl Template {
  /// Parse `input`, allowing only the keys listed in `allowed`.
  ///
  /// Errors (unknown key, unbalanced brace) are hard errors listing the valid
  /// keys, consistent with the rest of the config validation policy.
  pub fn parse(input: &str, allowed: &[&str]) -> Result<Self> {
    let key_index: HashMap<&str, usize> =
      allowed.iter().enumerate().map(|(i, k)| (*k, i)).collect();

    let bytes = input.as_bytes();
    let mut segments: Vec<Segment> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    let list = || allowed.to_vec().join(", ");

    while i < bytes.len() {
      let b = bytes[i];
      if b == b'{' {
        // escaped {{
        if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
          buf.push('{');
          i += 2;
          continue;
        }
        // placeholder — find closing }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && bytes[j] != b'}' {
          j += 1;
        }
        if j >= bytes.len() {
          return Err(anyhow!(
            "format template has unbalanced '{{' (missing '}}'): valid keys are {}",
            list()
          ));
        }
        let key = &input[start..j];
        let key_trimmed = key.trim();
        let idx = key_index.get(key_trimmed).copied().ok_or_else(|| {
          anyhow!(
            "format template references unknown key '{{{}}}' (allowed: {})",
            key_trimmed,
            list()
          )
        })?;
        if !buf.is_empty() {
          segments.push(Segment::Literal(std::mem::take(&mut buf)));
        }
        segments.push(Segment::Placeholder(idx));
        i = j + 1;
      } else if b == b'}' {
        // escaped }}
        if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
          buf.push('}');
          i += 2;
          continue;
        }
        return Err(anyhow!(
          "format template has unbalanced '}}' (no matching '{{'): valid keys are {}",
          list()
        ));
      } else {
        // safe because we only index ASCII braces above; push the char.
        // Using byte slicing from the original str keeps multi-byte UTF-8 intact.
        let ch_start = i;
        // advance by one UTF-8 char
        let next = i + utf8_len(b);
        buf.push_str(&input[ch_start..next]);
        i = next;
      }
    }
    if !buf.is_empty() {
      segments.push(Segment::Literal(buf));
    }
    Ok(Self { segments })
  }

  /// Render the template. `values` is indexed by the placeholder index assigned
  /// during [`parse`](Self::parse) (position in `allowed`).
  ///
  /// Newlines / tabs in the rendered output are stripped — ratatui treats text
  /// as text and would clip across rows, so this is the only real injection
  /// surface.
  pub fn render(&self, values: &[&str]) -> String {
    let mut out = String::new();
    for seg in &self.segments {
      match seg {
        Segment::Literal(s) => out.push_str(s),
        Segment::Placeholder(idx) => {
          if let Some(v) = values.get(*idx) {
            out.push_str(v);
          }
        }
      }
    }
    out.retain(|c| c != '\n' && c != '\r' && c != '\t');
    out
  }
}

/// Number of bytes consumed by the UTF-8 codepoint whose first byte is `b`.
fn utf8_len(b: u8) -> usize {
  if b < 0x80 {
    1
  } else if b >> 5 == 0b110 {
    2
  } else if b >> 4 == 0b1110 {
    3
  } else {
    4
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const KEYS: &[&str] = &["state", "device", "volume"];

  #[test]
  fn parse_and_render_simple() {
    let t = Template::parse("{state} on {device} ({volume}%)", KEYS).unwrap();
    assert_eq!(
      t.render(&["Playing", "Living Room", "42"]),
      "Playing on Living Room (42%)"
    );
  }

  #[test]
  fn escapes_braces() {
    let t = Template::parse("{{state}} = {state}", KEYS).unwrap();
    assert_eq!(t.render(&["Playing", "", ""]), "{state} = Playing");
  }

  #[test]
  fn unknown_key_errors() {
    let err = Template::parse("{bogus}", KEYS).unwrap_err();
    assert!(err.to_string().contains("bogus"));
    assert!(err.to_string().contains("state"));
  }

  #[test]
  fn unbalanced_brace_errors() {
    assert!(Template::parse("{state", KEYS).is_err());
    assert!(Template::parse("state}", KEYS).is_err());
  }

  #[test]
  fn strips_newlines() {
    let t = Template::parse("{state}", KEYS).unwrap();
    // a placeholder value containing a newline is sanitized
    assert_eq!(t.render(&["Play\ning"]), "Playing");
  }

  #[test]
  fn empty_allowed_key_errors_cleanly() {
    let err = Template::parse("{x}", &[]).unwrap_err();
    assert!(err.to_string().contains("unknown key"));
  }

  #[test]
  fn literal_only() {
    let t = Template::parse("just text", KEYS).unwrap();
    assert_eq!(t.render(&[]), "just text");
  }
}
