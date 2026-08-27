//! Key derivation and frame decryption for the Qobuz stream, pure over byte slices.
//!
//! Session key = HKDF-SHA256(ikm = the 16 raw bytes of the hex secret, salt and
//! info from `session/start`), expanded to 16 bytes. Content key = AES-128-CBC
//! (PKCS7) decrypt of the wrapped key with the session key. Frames are
//! AES-128-CTR with a 64-bit big-endian counter and a nonce of the frame IV
//! followed by zero bytes.

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, KeyIvInit, StreamCipher};
use anyhow::{anyhow, Result};
use hkdf::Hkdf;
use sha2::Sha256;

type CbcDecryptor = cbc::Decryptor<aes::Aes128>;
type FrameCipher = ctr::Ctr64BE<aes::Aes128>;

/// Derive the 16-byte session key from the hex secret and the `infos` parts.
pub fn session_key(secret_hex: &str, salt: &[u8], info: &[u8]) -> Result<[u8; 16]> {
  let ikm = hex::decode(secret_hex).map_err(|e| anyhow!("Qobuz secret is not hex: {e}"))?;
  hkdf_16(&ikm, salt, info)
}

fn hkdf_16(ikm: &[u8], salt: &[u8], info: &[u8]) -> Result<[u8; 16]> {
  let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
  let mut okm = [0u8; 16];
  hk.expand(info, &mut okm)
    .map_err(|_| anyhow!("HKDF expand failed"))?;
  Ok(okm)
}

/// Unwrap the 16-byte content key (AES-128-CBC, PKCS7) with the session key.
pub fn unwrap_content_key(session_key: &[u8; 16], wrapped: &[u8], iv: &[u8]) -> Result<[u8; 16]> {
  let iv: &[u8; 16] = iv
    .try_into()
    .map_err(|_| anyhow!("content key IV is {} bytes, expected 16", iv.len()))?;
  let mut buf = wrapped.to_vec();
  let plain = CbcDecryptor::new(session_key.into(), iv.into())
    .decrypt_padded_mut::<Pkcs7>(&mut buf)
    .map_err(|e| anyhow!("content key unwrap failed: {e}"))?;
  plain
    .try_into()
    .map_err(|_| anyhow!("content key is {} bytes, expected 16", plain.len()))
}

/// Decrypt one frame in place with AES-128-CTR; `frame_iv` fills the nonce head.
pub fn decrypt_frame(content_key: &[u8; 16], frame_iv: &[u8], frame: &mut [u8]) -> Result<()> {
  if frame_iv.len() > 16 {
    return Err(anyhow!(
      "frame IV is {} bytes, expected at most 16",
      frame_iv.len()
    ));
  }
  let mut nonce = [0u8; 16];
  nonce[..frame_iv.len()].copy_from_slice(frame_iv);
  FrameCipher::new(content_key.into(), (&nonce).into()).apply_keystream(frame);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use aes::cipher::BlockEncryptMut;

  type CbcEncryptor = cbc::Encryptor<aes::Aes128>;

  #[test]
  fn hkdf_matches_rfc_5869_test_case_1() {
    let ikm = [0x0bu8; 22];
    let salt: Vec<u8> = (0u8..=0x0c).collect();
    let info: Vec<u8> = (0xf0u8..=0xf9).collect();
    let okm = hkdf_16(&ikm, &salt, &info).unwrap();
    assert_eq!(hex::encode(okm), "3cb25f25faacd57a90434f64d0362f2a");
  }

  #[test]
  fn session_key_decodes_the_hex_secret_as_ikm() {
    let key = session_key("000102030405060708090a0b0c0d0e0f", b"salt", b"info").unwrap();
    assert_eq!(hex::encode(key), "f83a387899f405fb64e48ee655b78972");
  }

  #[test]
  fn session_key_rejects_a_non_hex_secret() {
    assert!(session_key("not hex at all", b"salt", b"info").is_err());
  }

  #[test]
  fn unwrap_content_key_inverts_cbc_pkcs7_encryption() {
    let session = [0x11u8; 16];
    let iv = [0x22u8; 16];
    let content = [0x33u8; 16];
    let mut buf = [0u8; 32];
    buf[..16].copy_from_slice(&content);
    let wrapped = CbcEncryptor::new((&session).into(), (&iv).into())
      .encrypt_padded_mut::<Pkcs7>(&mut buf, 16)
      .unwrap()
      .to_vec();
    assert_eq!(wrapped.len(), 32);
    assert_eq!(
      unwrap_content_key(&session, &wrapped, &iv).unwrap(),
      content
    );
  }

  #[test]
  fn unwrap_content_key_rejects_a_short_iv() {
    assert!(unwrap_content_key(&[0u8; 16], &[0u8; 32], &[0u8; 8]).is_err());
  }

  #[test]
  fn decrypt_frame_is_an_involution_with_an_8_byte_iv() {
    let key = [0x44u8; 16];
    let iv = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let plain: Vec<u8> = (0u8..100).collect();
    let mut frame = plain.clone();
    decrypt_frame(&key, &iv, &mut frame).unwrap();
    assert_ne!(frame, plain);
    decrypt_frame(&key, &iv, &mut frame).unwrap();
    assert_eq!(frame, plain);
  }

  #[test]
  fn decrypt_frame_rejects_an_oversized_iv() {
    let mut frame = [0u8; 4];
    assert!(decrypt_frame(&[0u8; 16], &[0u8; 17], &mut frame).is_err());
  }
}
