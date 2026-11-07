// Evita console do Windows no build de release.
#![cfg_attr(all(not(debug_assertions), not(feature = "console")), windows_subsystem = "windows")]

fn main() {
    c2_desktop_lib::run()
}
