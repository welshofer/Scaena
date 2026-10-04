//! The web player's and editor's pages as `just web` builds them (PLAN 2.1–2.9), which it copies
//! to `pages/dist` and the build gzips into the binary (ADR-0012). A crate built before them
//! carries none, and `scaena serve` says to build them.

#[cfg(pages)]
include!(concat!(env!("OUT_DIR"), "/pages.rs"));

/// Each page's file by its path in the build, gzipped.
#[cfg(not(pages))]
static PAGES: &[(&str, &[u8])] = &[];

/// Whether this build carries the pages.
pub fn built() -> bool {
    !PAGES.is_empty()
}

/// The page's file at `path` in the build, gzipped.
pub fn gzipped(path: &str) -> Option<&'static [u8]> {
    PAGES.iter().find(|(p, _)| *p == path).map(|(_, bytes)| *bytes)
}

/// What a file is, by its name, for `Content-Type`.
pub fn media_type(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase());
    match ext.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("scn" | "txt" | "md") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
