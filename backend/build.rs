use std::fs;
use std::path::Path;

fn main() {
    let dist = Path::new("../frontend/dist");
    if !dist.exists() {
        fs::create_dir_all(dist).expect("create frontend dist directory");
    }
    let index = dist.join("index.html");
    if !index.exists() {
        fs::write(
            index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Receiver</title></head><body><div id=\"root\">Frontend has not been built yet.</div></body></html>",
        )
        .expect("write fallback frontend index");
    }
    println!("cargo:rerun-if-changed=../frontend/dist");
}
