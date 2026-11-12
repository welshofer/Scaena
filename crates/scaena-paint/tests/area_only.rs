//! vello's two multisampled fine shaders are embedded as no text (vendor/vello_shaders/SCAENA.md,
//! ADR-0004 finding 31), which holds only while every vello renderer Scaena makes asks for area
//! antialiasing alone: the GPU painter, natively and on the Mac's layer, and the browser's WebGPU
//! canvas. A renderer that asked for MSAA would hand wgpu an empty shader.

use std::path::{Path, PathBuf};

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|x| x == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn every_vello_renderer_asks_for_area_antialiasing_alone() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let src = entry.unwrap().path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let (mut renderers, mut supports, mut methods) = (Vec::new(), 0, 0);
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        let name = file.strip_prefix(&root).unwrap().display().to_string();
        renderers.extend(text.matches("vello::RendererOptions {").map(|_| name.clone()));
        for line in text.lines().map(str::trim) {
            if let Some(support) = line.strip_prefix("antialiasing_support:") {
                assert_eq!(support.trim(), "vello::AaSupport::area_only(),", "{name}: {line}");
                supports += 1;
            }
            if let Some(method) = line.strip_prefix("antialiasing_method:") {
                assert_eq!(method.trim(), "vello::AaConfig::Area,", "{name}: {line}");
                methods += 1;
            }
            assert!(!line.contains("AaSupport::all") && !line.contains("Msaa"), "{name}: {line}");
        }
    }
    renderers.sort();
    assert_eq!(
        renderers,
        ["crates/scaena-paint/src/lib.rs", "crates/scaena-wasm/src/lib.rs"],
        "the renderers Scaena makes"
    );
    assert_eq!(supports, renderers.len(), "each renderer says the antialiasing it supports");
    assert!(methods >= renderers.len(), "each renders by area: {methods}");

    // And the build leaves out exactly the shaders no renderer asks for.
    let build = std::fs::read_to_string(root.join("vendor/vello_shaders/build.rs")).unwrap();
    assert!(
        build.contains(r#"const UNASKED: [&str; 2] = ["fine_msaa16", "fine_msaa8"];"#),
        "the multisampled fine shaders are left out"
    );
}
