// SPDX-License-Identifier: GPL-3.0-or-later

//! Translation engine bindings for dragomand.
//!
//! Wraps the C ABI of the Bergamot adapter (`engine/adapter/`) in a safe
//! API ([`bergamot`], behind the default `bergamot` feature) and runs
//! inference on dedicated [`worker`] threads. The [`fake`] backend lets
//! higher layers test without the C++ engine or model files.

pub mod backend;
pub mod fake;
pub mod worker;

#[cfg(feature = "bergamot")]
mod ffi;

#[cfg(feature = "bergamot")]
pub mod bergamot;

pub use backend::{Backend, Error, ModelFiles, Result, TranslateOptions};
pub use worker::{ModelSpec, Worker};

/// The Marian options Firefox passes, minus file paths (those travel in
/// [`ModelFiles`]). `model_file_name` picks the gemm precision: models named
/// `*.intgemm8.bin` use `int8shiftAll`, everything else (Mozilla currently
/// ships only `*.intgemm.alphas.bin`) uses `int8shiftAlphaAll`.
/// See docs/model-compatibility.md.
pub fn marian_config(model_file_name: &str) -> String {
    let gemm_precision = if model_file_name.ends_with("intgemm8.bin") {
        "int8shiftAll"
    } else {
        "int8shiftAlphaAll"
    };
    format!(
        "beam-size: 1\n\
         normalize: 1.0\n\
         word-penalty: 0\n\
         max-length-break: 128\n\
         mini-batch-words: 1024\n\
         workspace: 128\n\
         max-length-factor: 2.0\n\
         skip-cost: true\n\
         cpu-threads: 0\n\
         quiet: true\n\
         quiet-translation: true\n\
         gemm-precision: {gemm_precision}\n\
         alignment: soft\n"
    )
}
