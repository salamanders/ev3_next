use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use flate2::write::GzEncoder;
use flate2::Compression;

fn compress_file(src_path: &Path, dst_path: &Path) {
    let mut input = Vec::new();
    let mut file = File::open(src_path).unwrap_or_else(|e| {
        panic!("Failed to open {}: {}", src_path.display(), e);
    });
    file.read_to_end(&mut input).unwrap_or_else(|e| {
        panic!("Failed to read {}: {}", src_path.display(), e);
    });

    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&input).unwrap_or_else(|e| {
        panic!("Failed to compress {}: {}", src_path.display(), e);
    });
    let compressed = encoder.finish().unwrap_or_else(|e| {
        panic!("Failed to finish compression for {}: {}", src_path.display(), e);
    });

    std::fs::write(dst_path, &compressed).unwrap_or_else(|e| {
        panic!("Failed to write {}: {}", dst_path.display(), e);
    });
}

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);
    let web_assets_dir = Path::new("web_assets");

    let files = ["index.html", "design.html", "run.html", "style.css", "app.js"];

    for filename in &files {
        let src = web_assets_dir.join(filename);
        println!("cargo:rerun-if-changed={}", src.display());

        let dst_out = out_path.join(format!("{}.gz", filename));
        compress_file(&src, &dst_out);
    }

    println!("cargo:rerun-if-changed=build.rs");
}
