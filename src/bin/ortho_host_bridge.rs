//! Stdio JSON-lines host daemon exposing Ortho Editor's Tauri-shaped [`HostSessionStore`].

fn main() {
    if let Err(err) = d2r_core::projection::host::run_stdio_loop() {
        eprintln!("ortho_host_bridge failed: {err}");
        std::process::exit(1);
    }
}
