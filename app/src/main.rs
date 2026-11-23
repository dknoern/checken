//! Checken: a small personal-finance desktop app. Keep accounts, enter
//! transactions in a register, plan a monthly budget and read reports.

// On Windows, start as a windowed app (no black console window behind it).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod icon;
mod import;
mod model;
mod plaid;
mod qfx;

/// Names the running app "Checken" for macOS (the Dock and menu bar), in case
/// the system remembered the program file under another spelling.
#[cfg(target_os = "macos")]
fn set_app_name() {
    use objc2_foundation::{NSProcessInfo, NSString};
    NSProcessInfo::processInfo().setProcessName(&NSString::from_str("Checken"));
}

#[cfg(not(target_os = "macos"))]
fn set_app_name() {}

fn main() -> eframe::Result {
    set_app_name();
    // `--write-icon <file.png>` saves the app icon as a 1024-pixel picture and exits.
    let args: Vec<String> = std::env::args().collect();
    if let Some(path) = args.iter().position(|a| a == "--write-icon").and_then(|i| args.get(i + 1)) {
        let size = 1024;
        let ok = image::RgbaImage::from_raw(size, size, icon::pixels(size)).is_some_and(|img| img.save(path).is_ok());
        std::process::exit(if ok { 0 } else { 1 });
    }
    // `--backup <file.json>` saves a backup of all the data and exits.
    if let Some(path) = args.iter().position(|a| a == "--backup").and_then(|i| args.get(i + 1)) {
        match model::Ledger::load().save_to(std::path::Path::new(path)) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("couldn't save the backup: {e}");
                std::process::exit(1);
            }
        }
    }
    app::run()
}
