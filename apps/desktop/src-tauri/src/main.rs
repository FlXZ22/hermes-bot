#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(target_os = "linux")]
    if args.first().is_some_and(|arg| arg == "--relaunch-after") {
        let result = match args.as_slice() {
            [_, pid, target] => pid.to_str().and_then(|pid| pid.parse::<libc::pid_t>().ok())
                .filter(|pid| *pid > 1)
                .ok_or_else(|| "Invalid parent PID".to_string())
                .and_then(|pid| relaunch_after(pid, target)),
            _ => Err("Invalid relaunch arguments".into()),
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|arg| arg == "--diagnose-backend") {
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

#[cfg(target_os = "linux")]
fn relaunch_after(parent: libc::pid_t, target: &std::ffi::OsStr) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let started = std::time::Instant::now();
    loop {
        if unsafe { libc::kill(parent, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            return Err(std::process::Command::new(target).exec().to_string());
        }
        if started.elapsed() >= std::time::Duration::from_secs(15) {
            return Err("The previous Tauri instance did not exit.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(75));
    }
}
