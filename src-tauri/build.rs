fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Reserve virtual stack space for native WebView/COM callbacks; pages are committed on demand.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg=/STACK:8388608");
        } else {
            println!("cargo:rustc-link-arg=-Wl,--stack,8388608");
        }
    }
    tauri_build::build()
}
