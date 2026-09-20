#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    ksd_decrypt_lib::run();
}
