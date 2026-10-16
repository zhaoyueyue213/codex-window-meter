#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod overlay;
mod quota;

fn main() {
    if std::env::args().any(|argument| argument == "--diagnose") {
        println!("{}", overlay::diagnose());
        return;
    }
    if let Err(error) = overlay::run() {
        eprintln!("Codex Window Meter: {error}");
    }
}
