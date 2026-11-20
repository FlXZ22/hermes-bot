#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|arg| arg == "--diagnose-backend") {
        if let Err(error) = hermes_grok_bot_lib::diagnose_backend() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    // On Wayland + NVIDIA (e.g. Arch Linux / Omarchy + Hyprland + RTX GPUs),
    // WebKitGTK 4.1's DMABUF renderer can cause blank surfaces or EGL errors.
    // Default it safely unless the user explicitly configured it.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    hermes_grok_bot_lib::run();
}
