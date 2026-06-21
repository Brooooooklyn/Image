#![deny(clippy::all)]

pub mod avif;
mod fast_resize;
pub mod jpeg;
// P3: deterministic integer sRGB->CIELAB + CIE76 ΔE, wired into the quantizer's
// perceptual color ASSIGNMENT metric (`pdist` in `quantize.rs`).
mod lab;
pub mod png;
mod quantize;
pub mod transformer;
mod utils;
mod webp;
