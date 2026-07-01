// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    if photyx_lib::constants::FORCE_X11_ON_LINUX {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    photyx_lib::run()
}
