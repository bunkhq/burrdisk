fn main() {
    // On Windows, give the executable its icon and a manifest (per-monitor DPI
    // awareness, long paths, modern common controls)
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/windows");
        embed_resource::compile("assets/windows/burrdisk.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
