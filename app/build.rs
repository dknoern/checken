//! Per-system touches at build time:
//! - Mac: builds the app's name into the program. macOS reads an embedded
//!   Info.plist (the `__TEXT,__info_plist` section) for the name the Dock and
//!   the menu bar show, so they say "Checken" rather than whatever name the
//!   system last recorded for the program file.
//! - Windows: when CHECKEN_WINDOWS_ICON names an .ico file (the packaging
//!   script makes one from assets/icon.png), it's built into Checken.exe as
//!   its icon, through a Windows resource compiled with windres.

fn main() {
    println!("cargo:rerun-if-changed=macos/Info.plist");
    println!("cargo:rerun-if-env-changed=CHECKEN_WINDOWS_ICON");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos/Info.plist");
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
    if target_os == "windows" {
        if let Ok(ico) = std::env::var("CHECKEN_WINDOWS_ICON") {
            println!("cargo:rerun-if-changed={ico}");
            let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
            let rc = out.join("checken.rc");
            let obj = out.join("checken-icon.o");
            let _ = std::fs::write(&rc, format!("1 ICON \"{}\"\n", ico.replace('\\', "/")));
            let ok = std::process::Command::new("x86_64-w64-mingw32-windres")
                .arg(&rc)
                .arg("-O")
                .arg("coff")
                .arg("-o")
                .arg(&obj)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                println!("cargo:rustc-link-arg-bins={}", obj.display());
            } else {
                println!("cargo:warning=Couldn't build the Windows icon resource; Checken.exe will have the default icon.");
            }
        }
    }
}
