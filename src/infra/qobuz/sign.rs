//! Request signature for the signed Qobuz endpoints.
//!
//! `request_sig` is the lowercase hex MD5 of `<object><method><args><ts><secret>`,
//! where `<object><method>` is the endpoint with its slash removed and `<args>`
//! is every query argument as `key` + `value`, keys sorted, no separators.

use md5::{Digest, Md5};

/// The `request_sig` value for `endpoint` (for example `"session/start"`).
pub fn request_sig(endpoint: &str, args: &[(&str, &str)], request_ts: u64, secret: &str) -> String {
  let mut sorted: Vec<&(&str, &str)> = args.iter().collect();
  sorted.sort_by(|a, b| a.0.cmp(b.0));
  let mut input = endpoint.replace('/', "");
  for (key, value) in sorted {
    input.push_str(key);
    input.push_str(value);
  }
  input.push_str(&request_ts.to_string());
  input.push_str(secret);
  hex::encode(Md5::digest(input.as_bytes()))
}

#[cfg(test)]
mod tests {
  use super::*;

  const SECRET: &str = "abcdef0123456789abcdef0123456789";

  #[test]
  fn session_start_signature_matches_precomputed_md5() {
    let sig = request_sig(
      "session/start",
      &[("profile", "qbz-1")],
      1_700_000_000,
      SECRET,
    );
    assert_eq!(sig, "37a4a7a6ca79344aaf11aa0d3b05e677");
  }

  #[test]
  fn arguments_are_sorted_by_key_before_hashing() {
    let ordered = request_sig(
      "file/url",
      &[
        ("format_id", "27"),
        ("intent", "stream"),
        ("track_id", "123"),
      ],
      1_700_000_000,
      SECRET,
    );
    let reversed = request_sig(
      "file/url",
      &[
        ("track_id", "123"),
        ("intent", "stream"),
        ("format_id", "27"),
      ],
      1_700_000_000,
      SECRET,
    );
    assert_eq!(ordered, "7ea61d336e3ff5c0c46b3d984ad36834");
    assert_eq!(reversed, ordered);
  }
}
