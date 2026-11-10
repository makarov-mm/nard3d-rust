//! 3D Nardy: long and short nardy (backgammon) against a neural network, on OpenGL 4.6.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod app;
mod board_model;
mod board_view;
mod camera;
mod gfx;
mod math;
mod platform;
mod renderer;
mod settings;
mod shaders;
mod sound;
mod strings;
mod text;
mod themes;
mod ui;

// The rules, the network and the random numbers are the library's, which the trainer of the networks shares.
use nardy3d::{nardy, network, random};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = app::run() {
        platform::show_error("3D Nardy", &error.to_string());
        std::process::exit(1);
    }
}
