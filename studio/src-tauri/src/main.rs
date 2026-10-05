// Sem a janela de console no Windows, na build de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lace_studio_lib::run();
}
