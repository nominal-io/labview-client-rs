// Module declarations only — see CLAUDE.md repository layout.
//
// Every exported function takes raw pointers — that's the C ABI LabVIEW
// calls. Clippy wants such functions marked `unsafe`, but they are never
// called from Rust (tests aside), every pointer is null-checked before use,
// and an `unsafe` marker would be invisible to the C header anyway. Keeping
// them "safe" keeps call sites and tests free of unsafe-block noise.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod asset;
pub mod channel;
pub mod client;
pub mod dataset;
pub mod error;
pub mod handles;
pub mod run;
pub mod runtime;
pub mod strings;
pub mod video;
