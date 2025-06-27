#![deny(clippy::all)]

#[cfg(not(any(target_family = "wasm", target_arch = "arm")))]
#[global_allocator]
static ALLOC: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

pub mod avif;
mod fast_resize;
pub mod jpeg;
pub mod png;
pub mod transformer;
mod utils;
mod webp;
