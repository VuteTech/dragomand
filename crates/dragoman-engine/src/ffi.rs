// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written bindings for `engine/adapter/dragoman_engine.h` (ABI v1).
//! Keep in exact sync with the header; it is small by design.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int};

macro_rules! opaque {
    ($name:ident) => {
        #[repr(C)]
        pub struct $name {
            _data: [u8; 0],
            _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
        }
    };
}

opaque!(dg_engine);
opaque!(dg_model);
opaque!(dg_result);
opaque!(dg_error);

#[repr(C)]
pub struct dg_engine_options {
    pub cache_size: usize,
    pub log_level: *const c_char,
}

#[repr(C)]
pub struct dg_model_files {
    pub model_path: *const c_char,
    pub vocab_path: *const c_char,
    pub src_vocab_path: *const c_char,
    pub trg_vocab_path: *const c_char,
    pub shortlist_path: *const c_char,
}

#[repr(C)]
pub struct dg_text {
    pub data: *const c_char,
    pub len: usize,
}

#[repr(C)]
pub struct dg_translate_options {
    pub html: c_int,
}

unsafe extern "C" {
    pub fn dg_engine_abi_version() -> usize;

    pub fn dg_engine_create(
        options: *const dg_engine_options,
        err: *mut *mut dg_error,
    ) -> *mut dg_engine;
    pub fn dg_engine_destroy(engine: *mut dg_engine);

    pub fn dg_model_load(
        engine: *mut dg_engine,
        files: *const dg_model_files,
        config_yaml: *const c_char,
        err: *mut *mut dg_error,
    ) -> *mut dg_model;
    pub fn dg_model_unload(model: *mut dg_model);

    pub fn dg_translate_batch(
        engine: *mut dg_engine,
        first: *mut dg_model,
        second: *mut dg_model,
        segments: *const dg_text,
        n: usize,
        options: *const dg_translate_options,
        out: *mut *mut dg_result,
        err: *mut *mut dg_error,
    ) -> c_int;

    pub fn dg_result_len(result: *const dg_result) -> usize;
    pub fn dg_result_text(result: *const dg_result, index: usize) -> dg_text;
    pub fn dg_result_free(result: *mut dg_result);

    pub fn dg_error_message(err: *const dg_error) -> *const c_char;
    pub fn dg_error_free(err: *mut dg_error);
}
