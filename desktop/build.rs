fn main() {
  // A release without the built page would ship the server's 503 page.
  if std::env::var("PROFILE").as_deref() == Ok("release")
    && !std::path::Path::new("../gui/dist/index.html").exists()
  {
    panic!("gui/dist is missing: run `npm ci && npm run build` in gui/ first");
  }
  println!("cargo:rerun-if-changed=../gui/dist/index.html");
  tauri_build::build();
}
