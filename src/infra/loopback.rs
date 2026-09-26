//! Secrets shared by the loopback listeners (the MCP control socket, the GUI page server).

use anyhow::{Context, Result};

pub fn generate_token() -> Result<String> {
  // 128 bits straight from the OS CSPRNG, hex-encoded. `SysRng` rather than the
  // thread RNG so the entropy source is the one the comment claims, with no
  // userspace state to reason about for a value that authenticates a socket.
  use rand::rngs::SysRng;
  use rand::TryRng;
  let mut rng = SysRng;
  let high = rng
    .try_next_u64()
    .context("could not read from the system random source")?;
  let low = rng
    .try_next_u64()
    .context("could not read from the system random source")?;
  Ok(format!("{high:016x}{low:016x}"))
}

/// Compares without exiting early at the first differing byte.
pub fn tokens_match(presented: &str, expected: &str) -> bool {
  presented.len() == expected.len()
    && presented
      .bytes()
      .zip(expected.bytes())
      .fold(0u8, |acc, (a, b)| acc | (a ^ b))
      == 0
}
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generated_tokens_are_long_and_distinct() {
    let a = generate_token().unwrap();
    let b = generate_token().unwrap();
    assert_eq!(a.len(), 32, "128 bits, hex-encoded");
    assert_ne!(a, b);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
  }
}
