//! Embeds the built frontend (`gui/dist`) into a `gui` build. It writes only into
//! OUT_DIR: `cargo publish` rejects a build script that changes the source tree.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
  println!("cargo:rerun-if-changed=build.rs");
  if env::var_os("CARGO_FEATURE_GUI").is_none() {
    return;
  }
  let dist = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
    .join("gui")
    .join("dist");
  // Watched only when present: a missing path would rerun the script on every build.
  if dist.is_dir() {
    println!("cargo:rerun-if-changed={}", dist.display());
  }
  let mut files = Vec::new();
  collect(&dist, &dist, &mut files);
  files.sort();
  let mut table = String::from("&[\n");
  for (route, path) in &files {
    table.push_str(&format!(
      "  ({route:?}, include_bytes!({path:?}) as &[u8]),\n"
    ));
  }
  table.push(']');
  fs::write(
    PathBuf::from(env::var("OUT_DIR").unwrap()).join("gui_assets.rs"),
    table,
  )
  .unwrap();
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
  let Ok(entries) = fs::read_dir(dir) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect(root, &path, files);
    } else if let Ok(relative) = path.strip_prefix(root) {
      files.push((
        relative.to_string_lossy().replace('\\', "/"),
        path.to_string_lossy().into_owned(),
      ));
    }
  }
}
