//! Parsers for the Qobuz CMAF segments, pure over `&[u8]`.
//!
//! Segment 0 carries a `uuid` box ([`INIT_UUID`]) with the raw FLAC header and a
//! table of per-segment byte lengths. Every later segment carries a `uuid` box
//! ([`SEGMENT_UUID`]) with one entry per frame (length, flags, IV) and the frames
//! themselves at `data_offset` from the box start. Only frames whose flags are
//! not zero are encrypted.

use anyhow::{anyhow, Result};

use super::crypto::decrypt_frame;

/// Box type of the init payload.
pub const INIT_UUID: [u8; 16] = [
  0xc7, 0xc7, 0x5d, 0xf0, 0xfd, 0xd9, 0x51, 0xe9, 0x8f, 0xc2, 0x29, 0x71, 0xe4, 0xac, 0xf8, 0xd2,
];
/// Box type of the per-segment frame table.
pub const SEGMENT_UUID: [u8; 16] = [
  0x3b, 0x42, 0x12, 0x92, 0x56, 0xf3, 0x5f, 0x75, 0x92, 0x36, 0x63, 0xb6, 0x9a, 0x1f, 0x52, 0xb2,
];

/// The parsed init segment.
#[derive(Debug, Clone, PartialEq)]
pub struct InitSegment {
  /// The codec header to write first (a FLAC header with the last-block bit set).
  pub header: Vec<u8>,
  /// Byte length of each audio segment, in order (segment 1 first).
  pub segment_lengths: Vec<u32>,
}

impl InitSegment {
  /// The size of the finished file: header plus every segment.
  pub fn total_bytes(&self) -> u64 {
    self.header.len() as u64 + self.segment_lengths.iter().map(|&l| l as u64).sum::<u64>()
  }
}

/// One frame entry of an audio segment.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameEntry {
  pub len: usize,
  pub flags: u16,
  pub iv: Vec<u8>,
}

/// The frame table of an audio segment.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentTable {
  /// Offset of the first frame in the segment buffer.
  pub frames_start: usize,
  pub frames: Vec<FrameEntry>,
}

impl SegmentTable {
  /// Offset one past the last frame.
  pub fn frames_end(&self) -> usize {
    self.frames_start + self.frames.iter().map(|f| f.len).sum::<usize>()
  }
}

struct Reader<'a> {
  buf: &'a [u8],
  pos: usize,
}

impl<'a> Reader<'a> {
  fn take(&mut self, n: usize) -> Result<&'a [u8]> {
    let end = self
      .pos
      .checked_add(n)
      .filter(|&end| end <= self.buf.len())
      .ok_or_else(|| anyhow!("segment truncated at byte {}", self.pos))?;
    let out = &self.buf[self.pos..end];
    self.pos = end;
    Ok(out)
  }

  fn be(&mut self, n: usize) -> Result<u64> {
    Ok(
      self
        .take(n)?
        .iter()
        .fold(0u64, |acc, &b| (acc << 8) | b as u64),
    )
  }

  fn u8(&mut self) -> Result<u8> {
    Ok(self.be(1)? as u8)
  }

  fn u16(&mut self) -> Result<u16> {
    Ok(self.be(2)? as u16)
  }

  fn u32(&mut self) -> Result<u32> {
    Ok(self.be(4)? as u32)
  }
}

/// Start offset and payload offset of the top-level `uuid` box with type `uuid`.
fn find_uuid_box(buf: &[u8], uuid: &[u8; 16]) -> Result<(usize, usize)> {
  let mut pos = 0usize;
  while pos.checked_add(8).is_some_and(|end| end <= buf.len()) {
    let mut r = Reader { buf, pos };
    let size32 = r.u32()?;
    let kind = r.take(4)?;
    let remaining = buf.len() - pos;
    let (header_len, size) = match size32 {
      0 => (8usize, remaining),
      1 => (16usize, usize::try_from(r.be(8)?).unwrap_or(usize::MAX)),
      n => (8usize, n as usize),
    };
    // A box that runs past the buffer cannot hold the target; this also keeps
    // `pos + size` from overflowing.
    if size < header_len || size > remaining {
      return Err(anyhow!("invalid box size {size} at byte {pos}"));
    }
    if kind == b"uuid" && r.take(16).is_ok_and(|found| found == uuid) {
      // Payload = box header, 16-byte type, 4 bytes of version and flags.
      return Ok((pos, pos + header_len + 16 + 4));
    }
    pos += size;
  }
  Err(anyhow!("no {} box in segment", hex::encode(uuid)))
}

/// Parse segment 0 into the codec header and the segment length table.
pub fn parse_init(buf: &[u8]) -> Result<InitSegment> {
  let (_, payload) = find_uuid_box(buf, &INIT_UUID)?;
  let mut r = Reader { buf, pos: payload };
  let _track_id = r.u32()?;
  let _file_id = r.u32()?;
  let _sample_rate = r.u32()?;
  let _bits_per_sample = r.u8()?;
  let _channels = r.take(3)?;
  let _samples_count = r.be(6)?;
  let header_len = r.u16()? as usize;
  let mut header = r.take(header_len)?.to_vec();
  let key_id_len = r.u8()? as usize;
  let _key_id = r.take(key_id_len)?;
  let segment_count = r.u16()? as usize;
  let mut segment_lengths = Vec::with_capacity(segment_count);
  for _ in 0..segment_count {
    segment_lengths.push(r.u32()?);
    let _samples = r.u32()?;
  }
  mark_last_metadata_block(&mut header)?;
  Ok(InitSegment {
    header,
    segment_lengths,
  })
}

/// Set the last-metadata-block flag on the final block of a FLAC header.
fn mark_last_metadata_block(header: &mut [u8]) -> Result<()> {
  if !header.starts_with(b"fLaC") {
    return Ok(());
  }
  let mut pos = 4usize;
  loop {
    let block = header
      .get(pos..pos + 4)
      .ok_or_else(|| anyhow!("FLAC header truncated at byte {pos}"))?;
    if block[0] & 0x80 != 0 {
      return Ok(());
    }
    let len = ((block[1] as usize) << 16) | ((block[2] as usize) << 8) | block[3] as usize;
    let next = pos + 4 + len;
    if next >= header.len() {
      header[pos] |= 0x80;
      return Ok(());
    }
    pos = next;
  }
}

/// Parse the frame table of an audio segment (segments 1 and later).
pub fn parse_segment(buf: &[u8]) -> Result<SegmentTable> {
  let (box_start, payload) = find_uuid_box(buf, &SEGMENT_UUID)?;
  let mut r = Reader { buf, pos: payload };
  let data_offset = r.u32()? as usize;
  let iv_size = r.u8()? as usize;
  let frame_count = r.be(3)? as usize;
  let mut frames = Vec::with_capacity(frame_count.min(4096));
  for _ in 0..frame_count {
    let len = r.u32()? as usize;
    let _reserved = r.u16()?;
    let flags = r.u16()?;
    let iv = r.take(iv_size)?.to_vec();
    frames.push(FrameEntry { len, flags, iv });
  }
  let table = SegmentTable {
    frames_start: box_start + data_offset,
    frames,
  };
  if table.frames_end() > buf.len() {
    return Err(anyhow!(
      "frame table ends at byte {} but the segment holds {} bytes",
      table.frames_end(),
      buf.len()
    ));
  }
  Ok(table)
}

/// Decrypt every flagged frame in place; returns the byte range of all frames.
pub fn decrypt_frames(
  buf: &mut [u8],
  table: &SegmentTable,
  key: &[u8; 16],
) -> Result<std::ops::Range<usize>> {
  let mut pos = table.frames_start;
  for (i, frame) in table.frames.iter().enumerate() {
    let end = pos + frame.len;
    let data = buf
      .get_mut(pos..end)
      .ok_or_else(|| anyhow!("frame {i} runs past the segment"))?;
    if frame.flags != 0 {
      decrypt_frame(key, &frame.iv, data)?;
    }
    pos = end;
  }
  Ok(table.frames_start..pos)
}

/// Segment builders in the live byte layout, shared with the download tests.
#[cfg(test)]
pub(super) mod fixtures {
  use super::*;

  pub fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out
  }

  pub fn uuid_box(uuid: &[u8; 16], payload: &[u8]) -> Vec<u8> {
    let mut body = uuid.to_vec();
    body.extend_from_slice(&[0, 0, 0, 0]);
    body.extend_from_slice(payload);
    mp4_box(b"uuid", &body)
  }

  /// A 42-byte `fLaC` + STREAMINFO header without the last-block bit.
  pub fn flac_header() -> Vec<u8> {
    let mut h = b"fLaC".to_vec();
    h.extend_from_slice(&[0x00, 0, 0, 34]);
    h.extend((0u8..34).map(|i| i.wrapping_mul(7)));
    h
  }

  pub fn init_payload(header: &[u8], lengths: &[u32]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&7u32.to_be_bytes()); // track_id
    p.extend_from_slice(&9u32.to_be_bytes()); // file_id
    p.extend_from_slice(&96_000u32.to_be_bytes());
    p.push(24); // bits
    p.extend_from_slice(&[2, 0, 0]); // channels + padding
    p.extend_from_slice(&1_234_567u64.to_be_bytes()[2..]); // 6-byte samples
    p.extend_from_slice(&(header.len() as u16).to_be_bytes());
    p.extend_from_slice(header);
    p.push(3);
    p.extend_from_slice(b"kid");
    p.extend_from_slice(&(lengths.len() as u16).to_be_bytes());
    for (i, len) in lengths.iter().enumerate() {
      p.extend_from_slice(&len.to_be_bytes());
      p.extend_from_slice(&(4096 * (i as u32 + 1)).to_be_bytes());
    }
    p
  }

  /// Segment 0: an `ftyp` box then the init `uuid` box.
  pub fn init_segment(header: &[u8], lengths: &[u32]) -> Vec<u8> {
    let mut seg = mp4_box(b"ftyp", b"cmfc");
    seg.extend(uuid_box(&INIT_UUID, &init_payload(header, lengths)));
    seg
  }

  /// An audio segment from `(plain bytes, flags, iv)` frames; flagged frames are
  /// encrypted with `key`. Returns the segment and the expected plain output.
  pub fn audio_segment(frames: &[(Vec<u8>, u16, [u8; 8])], key: &[u8; 16]) -> (Vec<u8>, Vec<u8>) {
    let mut table = Vec::new();
    let mut data = Vec::new();
    let mut plain = Vec::new();
    for (bytes, flags, iv) in frames {
      table.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
      table.extend_from_slice(&[0, 0]);
      table.extend_from_slice(&flags.to_be_bytes());
      table.extend_from_slice(iv);
      let mut on_wire = bytes.clone();
      if *flags != 0 {
        decrypt_frame(key, iv, &mut on_wire).unwrap();
      }
      data.extend_from_slice(&on_wire);
      plain.extend_from_slice(bytes);
    }
    let mut payload = Vec::new();
    // uuid box = 8 header + 16 uuid + 4 flags + 8 fixed + table; mdat header = 8.
    let uuid_len = 8 + 16 + 4 + 8 + table.len();
    payload.extend_from_slice(&((uuid_len + 8) as u32).to_be_bytes());
    payload.push(8);
    payload.extend_from_slice(&(frames.len() as u32).to_be_bytes()[1..]);
    payload.extend_from_slice(&table);
    let mut seg = mp4_box(b"moof", b"");
    seg.extend(uuid_box(&SEGMENT_UUID, &payload));
    seg.extend(mp4_box(b"mdat", &data));
    (seg, plain)
  }
}

#[cfg(test)]
mod tests {
  use super::fixtures::*;
  use super::*;

  fn init_segment(lengths: &[u32]) -> Vec<u8> {
    fixtures::init_segment(&flac_header(), lengths)
  }

  #[test]
  fn init_parses_header_and_length_table() {
    let init = parse_init(&init_segment(&[100, 200, 300])).unwrap();
    assert_eq!(init.header.len(), 42);
    assert_eq!(&init.header[..4], b"fLaC");
    assert_eq!(init.header[4], 0x80, "last-metadata-block bit is set");
    assert_eq!(&init.header[5..], &flac_header()[5..]);
    assert_eq!(init.segment_lengths, vec![100, 200, 300]);
    assert_eq!(init.total_bytes(), 642);
  }

  #[test]
  fn oversized_boxes_are_errors_without_panic() {
    // A largesize box (size32 == 1) claiming u64::MAX bytes.
    let mut huge = 1u32.to_be_bytes().to_vec();
    huge.extend_from_slice(b"free");
    huge.extend_from_slice(&u64::MAX.to_be_bytes());
    huge.extend(init_segment(&[1]));
    assert!(parse_init(&huge).is_err());
    // A plain box whose size runs past the buffer.
    let mut long = 4096u32.to_be_bytes().to_vec();
    long.extend_from_slice(b"free");
    long.extend(init_segment(&[1]));
    assert!(parse_init(&long).is_err());
    assert!(parse_segment(&huge).is_err());
  }

  #[test]
  fn init_leaves_an_already_final_block_alone() {
    let mut header = flac_header();
    header[4] = 0x80;
    let mut seg = mp4_box(b"ftyp", b"cmfc");
    seg.extend(uuid_box(&INIT_UUID, &init_payload(&header, &[1])));
    assert_eq!(parse_init(&seg).unwrap().header, header);
  }

  #[test]
  fn init_marks_the_last_of_several_metadata_blocks() {
    let mut header = flac_header();
    header.extend_from_slice(&[0x04, 0, 0, 2, 0xaa, 0xbb]); // VORBIS_COMMENT
    let mut seg = mp4_box(b"ftyp", b"cmfc");
    seg.extend(uuid_box(&INIT_UUID, &init_payload(&header, &[1])));
    let parsed = parse_init(&seg).unwrap().header;
    assert_eq!(parsed[4], 0x00);
    assert_eq!(parsed[42], 0x84);
  }

  #[test]
  fn init_leaves_a_non_flac_header_untouched() {
    let header = vec![0xff, 0xfb, 0x90, 0x00, 0x11, 0x22];
    let mut seg = mp4_box(b"ftyp", b"cmfc");
    seg.extend(uuid_box(&INIT_UUID, &init_payload(&header, &[1])));
    assert_eq!(parse_init(&seg).unwrap().header, header);
  }

  #[test]
  fn init_parses_a_300_entry_table() {
    let lengths: Vec<u32> = (1..=300).collect();
    let init = parse_init(&init_segment(&lengths)).unwrap();
    assert_eq!(init.segment_lengths, lengths);
  }

  #[test]
  fn truncated_init_returns_an_error_without_panic() {
    let full = init_segment(&[100, 200, 300]);
    for cut in 0..full.len() {
      assert!(parse_init(&full[..cut]).is_err(), "prefix of {cut} bytes");
    }
  }

  #[test]
  fn init_without_the_uuid_box_is_an_error() {
    let seg = mp4_box(b"ftyp", b"cmfc");
    assert!(parse_init(&seg).is_err());
  }

  const KEY: [u8; 16] = [0x5a; 16];

  #[test]
  fn segment_decrypts_flagged_frames_and_passes_others_through() {
    let frames = vec![
      ((0u8..40).collect(), 1u16, [1u8, 2, 3, 4, 5, 6, 7, 8]),
      ((100u8..130).collect(), 0u16, [0u8; 8]),
    ];
    let (mut seg, plain) = audio_segment(&frames, &KEY);
    let table = parse_segment(&seg).unwrap();
    assert_eq!(table.frames.len(), 2);
    assert_eq!(table.frames[0].flags, 1);
    assert_eq!(table.frames[0].iv, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(table.frames[1].len, 30);
    assert_ne!(
      &seg[table.frames_start..table.frames_start + 40],
      &plain[..40]
    );
    let range = decrypt_frames(&mut seg, &table, &KEY).unwrap();
    assert_eq!(&seg[range], &plain[..]);
  }

  #[test]
  fn truncated_segment_returns_an_error_without_panic() {
    let frames = vec![((0u8..40).collect(), 1u16, [9u8; 8])];
    let (full, _) = audio_segment(&frames, &KEY);
    for cut in 0..full.len() {
      assert!(
        parse_segment(&full[..cut]).is_err(),
        "prefix of {cut} bytes"
      );
    }
  }

  #[test]
  fn find_uuid_box_skips_a_box_with_a_different_uuid() {
    let other = [0x99u8; 16];
    let mut seg = uuid_box(&other, &[0; 8]);
    seg.extend(uuid_box(&INIT_UUID, &init_payload(&flac_header(), &[5])));
    assert_eq!(parse_init(&seg).unwrap().segment_lengths, vec![5]);
  }
}
