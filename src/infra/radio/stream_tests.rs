use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

async fn serve(status: &str, headers: &str, body: Vec<u8>) -> String {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("http://{}/station", listener.local_addr().unwrap());
  let response = format!(
    "{status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
    body.len()
  );
  tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut request = [0; 2048];
    socket.read(&mut request).await.unwrap();
    socket.write_all(response.as_bytes()).await.unwrap();
    socket.write_all(&body).await.unwrap();
  });
  url
}

#[tokio::test]
async fn a_playlist_tunes_in_to_the_lowest_numbered_pls_entry() {
  let direct = serve(
    "HTTP/1.1 200 OK",
    "Content-Type: audio/mpeg\r\nicy-name: Selected\r\n",
    vec![0x5a; 4096],
  )
  .await;
  let text = format!("[playlist]\rFile10=http://127.0.0.1:1/missing\rFile2={direct}.m3u\r");
  let mut body = vec![0xff, 0xfe];
  body.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
  let playlist = serve("HTTP/1.1 200 OK", "", body).await;
  let opened = open_radio_stream(&playlist).await.unwrap();
  assert_eq!(opened.station_name.as_deref(), Some("Selected"));
  assert_eq!(opened.content_type.as_deref(), Some("audio/mpeg"));
}

#[tokio::test]
async fn a_playlist_can_point_to_audio_behind_a_redirect() {
  let direct = serve(
    "HTTP/1.1 200 OK",
    "Content-Type: audio/mpeg\r\nicy-name: Redirected\r\n",
    vec![0x5a; 4096],
  )
  .await;
  let redirect = serve(
    "HTTP/1.1 302 Found",
    &format!("Location: {direct}.pls\r\n"),
    vec![],
  )
  .await;
  let playlist = serve(
    "HTTP/1.1 200 OK",
    "Content-Type: audio/x-mpegurl\r\n",
    format!("#EXTM3U\n{redirect}.m3u\n").into_bytes(),
  )
  .await;
  let opened = open_radio_stream(&playlist).await.unwrap();
  assert_eq!(opened.station_name.as_deref(), Some("Redirected"));
}

async fn serve_live(headers: &str) -> (String, JoinHandle<TcpStream>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("http://{}/live", listener.local_addr().unwrap());
  let response = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n");
  let server = tokio::spawn(async move {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut request = [0; 2048];
    socket.read(&mut request).await.unwrap();
    socket.write_all(response.as_bytes()).await.unwrap();
    socket
  });
  (url, server)
}

#[tokio::test]
async fn sniffing_an_ambiguous_audio_response_preserves_its_prefix() {
  let body: Vec<_> = (0..=255).cycle().take(24 * 1024).collect();
  let url = serve("HTTP/1.1 200 OK", "", body.clone()).await;
  let mut opened = open_radio_stream(&url).await.unwrap();
  let received = tokio::task::spawn_blocking(move || {
    let mut received = Vec::new();
    opened.reader.read_to_end(&mut received).unwrap();
    received
  })
  .await
  .unwrap();
  assert_eq!(received, body);
}

#[tokio::test]
async fn a_nested_hls_playlist_keeps_the_hls_error() {
  let hls = serve(
    "HTTP/1.1 200 OK",
    "",
    b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n".to_vec(),
  )
  .await;
  let playlist = serve(
    "HTTP/1.1 200 OK",
    "Content-Type: audio/x-mpegurl\r\n",
    format!("#EXTM3U\n{hls}\n").into_bytes(),
  )
  .await;
  let failure = open_radio_stream(&playlist).await.err().unwrap();
  assert!(matches!(failure.downcast_ref(), Some(RadioOpenError::Hls)));
}

#[tokio::test]
async fn oversized_nested_and_non_http_playlists_request_a_direct_stream() {
  let nested = serve(
    "HTTP/1.1 200 OK",
    "Content-Type: audio/x-mpegurl\r\n",
    b"#EXTM3U\nhttps://example.invalid/station\n".to_vec(),
  )
  .await;
  for body in [
    format!("#EXTM3U\n{nested}\n").into_bytes(),
    vec![b'#'; 64 * 1024 + 1],
    b"#EXTM3U\nfile:///station\n".to_vec(),
  ] {
    let url = serve("HTTP/1.1 200 OK", "Content-Type: audio/x-mpegurl\r\n", body).await;
    let failure = open_radio_stream(&url).await.err().unwrap();
    assert!(matches!(
      failure.downcast_ref(),
      Some(RadioOpenError::Playlist)
    ));
  }
}

#[tokio::test]
async fn an_invalid_http_status_line_gets_a_direct_stream_hint() {
  let url = serve("ICY 200 OK", "", vec![]).await;
  let failure = open_radio_stream(&url).await.err().unwrap();
  assert!(matches!(
    failure.downcast_ref(),
    Some(RadioOpenError::InvalidHttpResponse)
  ));
  let url = serve("HTTP/1.1 404 Not Found", "", vec![]).await;
  let failure = open_radio_stream(&url).await.err().unwrap();
  assert!(failure.to_string().contains("404"));
}

#[tokio::test]
async fn a_probe_on_a_slow_station_finishes_below_the_old_prefetch_size() {
  let (url, socket) = serve_live("Content-Type: audio/mpeg\r\nicy-br: 32\r\n").await;
  let sent = Arc::new(AtomicUsize::new(0));
  let sent_by_server = Arc::clone(&sent);
  let server = tokio::spawn(async move {
    let mut socket = socket.await.unwrap();
    let mut frame = [0; 104];
    // MPEG-1 Layer III, 32 kbit/s at 44.1 kHz.
    frame[..4].copy_from_slice(&[0xff, 0xfb, 0x10, 0x00]);
    let packet = frame.repeat(4);
    let mut interval = tokio::time::interval(Duration::from_millis(104));
    let mut disconnect = [0; 1];
    loop {
      tokio::select! {
        _ = socket.read(&mut disconnect) => break,
        _ = interval.tick() => {
          if socket.write_all(&packet).await.is_err() {
            break;
          }
          sent_by_server.fetch_add(packet.len(), Ordering::Relaxed);
        }
      }
    }
  });
  let opened = open_until(&url, Instant::now() + Duration::from_secs(8))
    .await
    .unwrap();
  let prepared = opened.prepare().await.map(drop);
  let sent_at_prepare = sent.load(Ordering::Relaxed);
  tokio::time::timeout(Duration::from_secs(2), server)
    .await
    .unwrap()
    .unwrap();
  // Let cancellation wake the blocking probe before an assertion can shut
  // down the test runtime.
  prepared.unwrap();
  assert!((12_000..64 * 1024).contains(&sent_at_prepare));
}

#[tokio::test]
async fn playlist_resolution_and_the_probe_spend_the_same_budget() {
  let (target, socket) = serve_live("Content-Type: audio/mpeg\r\n").await;
  let playlist = serve(
    "HTTP/1.1 200 OK",
    "Content-Type: audio/x-mpegurl\r\n",
    format!("#EXTM3U\n{target}\n").into_bytes(),
  )
  .await;
  let deadline = Instant::now() + Duration::from_millis(500);
  let opened = open_until(&playlist, deadline).await.unwrap();
  let failure = tokio::time::timeout(Duration::from_secs(2), opened.prepare())
    .await
    .unwrap()
    .err()
    .unwrap();
  assert!(matches!(
    failure.downcast_ref(),
    Some(RadioOpenError::ProbeTimeout)
  ));
  assert!(Instant::now() >= deadline);
  let mut socket = socket.await.unwrap();
  let mut closed = [0; 1];
  assert_eq!(
    tokio::time::timeout(Duration::from_secs(2), socket.read(&mut closed))
      .await
      .unwrap()
      .unwrap(),
    0
  );
}

#[tokio::test]
async fn a_playlist_that_never_ends_is_cancelled_at_the_deadline() {
  let (url, socket) = serve_live("Content-Type: audio/x-mpegurl\r\n").await;
  let failure = open_until(&url, Instant::now() + Duration::from_millis(350))
    .await
    .err()
    .unwrap();
  assert!(matches!(
    failure.downcast_ref(),
    Some(RadioOpenError::Playlist)
  ));
  let mut socket = socket.await.unwrap();
  let mut closed = [0; 1];
  assert_eq!(
    tokio::time::timeout(Duration::from_secs(2), socket.read(&mut closed))
      .await
      .unwrap()
      .unwrap(),
    0
  );
}

#[tokio::test]
async fn dropping_a_reader_behind_a_full_ring_closes_the_connection() {
  let (url, socket) = serve_live("Content-Type: audio/mpeg\r\n").await;
  let sent = Arc::new(AtomicUsize::new(0));
  let sent_by_server = Arc::clone(&sent);
  let server = tokio::spawn(async move {
    let mut socket = socket.await.unwrap();
    let chunk = [0x5a; 16 * 1024];
    while socket.write_all(&chunk).await.is_ok() {
      sent_by_server.fetch_add(chunk.len(), Ordering::Relaxed);
    }
  });
  let opened = open_until(&url, Instant::now() + Duration::from_secs(8))
    .await
    .unwrap();
  // Nothing reads, so the 512 KiB ring and then the socket buffers fill and
  // the server stops making progress.
  let mut last = 0;
  loop {
    tokio::time::sleep(Duration::from_millis(300)).await;
    let now = sent.load(Ordering::Relaxed);
    if now > 512 * 1024 && now == last {
      break;
    }
    last = now;
  }
  drop(opened);
  tokio::time::timeout(Duration::from_secs(3), server)
    .await
    .expect("the download must let go of the connection")
    .unwrap();
}
