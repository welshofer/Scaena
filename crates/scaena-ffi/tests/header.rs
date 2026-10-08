//! `include/scaena.h` is what cbindgen makes of this crate's code, as `docs/schema` is what the
//! typed model makes (ADR-0007): Swift imports the header, so a change to the ABI shows as a
//! reviewed diff in it. `SCAENA_BLESS=1` writes it again (`just bless`).

#[test]
fn the_header_is_what_cbindgen_makes_of_the_code() {
    let dir = env!("CARGO_MANIFEST_DIR");
    let config = cbindgen::Config::from_file(format!("{dir}/cbindgen.toml")).unwrap();
    let mut made = Vec::new();
    cbindgen::Builder::new().with_crate(dir).with_config(config).generate().unwrap().write(&mut made);
    let path = format!("{dir}/include/scaena.h");
    if std::env::var_os("SCAENA_BLESS").is_some() {
        std::fs::write(&path, &made).unwrap();
        return;
    }
    let held = std::fs::read(&path).unwrap_or_default();
    assert!(held == made, "include/scaena.h is not what cbindgen makes of the code: run `just bless`");
}
