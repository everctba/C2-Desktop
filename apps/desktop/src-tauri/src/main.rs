// Evita console do Windows no build de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    c2_desktop_lib::run()
}
