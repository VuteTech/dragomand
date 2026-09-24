// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe wrapper over the Bergamot C ABI (`engine/adapter/dragoman_engine.h`).
//!
//! An [`Engine`] and the [`Model`]s loaded on it belong together on one
//! worker thread: everything is `Send` but nothing is `Sync`, and all
//! operations take `&mut Engine`.

use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::sync::Arc;

use crate::backend::{Backend, Error, ModelFiles, Result, TranslateOptions};
use crate::ffi;

/// Owns the `dg_engine` and destroys it last: models hold an `Arc` to it, so
/// the engine outlives every model regardless of drop order.
struct EngineHandle {
    ptr: NonNull<ffi::dg_engine>,
}

// SAFETY: the engine pointer may move between threads (the C side has no
// thread affinity). All operations on a live engine go through `&mut Engine`
// on one thread; `EngineHandle` itself exposes nothing but its final Drop,
// which the last `Arc` owner runs exactly once with exclusive access.
unsafe impl Send for EngineHandle {}
// SAFETY: see above — `EngineHandle` has no `&self` operations at all, so
// shared references across threads (via `Arc`) cannot race.
unsafe impl Sync for EngineHandle {}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        // SAFETY: the pointer came from dg_engine_create and every model
        // holds an Arc to this handle, so at this point (last owner) all
        // models have already been unloaded.
        unsafe { ffi::dg_engine_destroy(self.ptr.as_ptr()) };
    }
}

/// A translation engine instance (one Bergamot `BlockingService`).
pub struct Engine {
    handle: Arc<EngineHandle>,
    /// Everything on an engine requires `&mut`; forbid `Sync` sharing.
    _not_sync: PhantomData<Cell<()>>,
}

/// One loaded translation model. Unloaded on drop.
pub struct Model {
    ptr: NonNull<ffi::dg_model>,
    engine: Arc<EngineHandle>,
    _not_sync: PhantomData<Cell<()>>,
}

// SAFETY: a model moves between threads together with its engine; all use
// goes through `&mut Engine` plus `&Model` on the owning thread, and
// unloading a model is valid from the thread that owns it.
unsafe impl Send for Model {}

impl Drop for Model {
    fn drop(&mut self) {
        // SAFETY: the pointer came from dg_model_load on the engine this
        // model's Arc keeps alive, and is dropped exactly once.
        unsafe { ffi::dg_model_unload(self.ptr.as_ptr()) };
        // `self.engine` drops after this, keeping the engine alive above.
        let _ = &self.engine;
    }
}

/// Converts and frees a stored `dg_error`. `err` may be null.
fn take_error(err: *mut ffi::dg_error, context: &str) -> String {
    if err.is_null() {
        return format!("{context}: engine reported no message");
    }
    // SAFETY: `err` was stored by the adapter, its message is a valid C
    // string until dg_error_free, which is called exactly once below.
    unsafe {
        let message = CStr::from_ptr(ffi::dg_error_message(err))
            .to_string_lossy()
            .into_owned();
        ffi::dg_error_free(err);
        message
    }
}

fn path_to_cstring(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| Error::InvalidInput(format!("path contains NUL: {}", path.display())))
}

impl Engine {
    /// Creates an engine with `cache_size` cached sentences (0 disables).
    pub fn new(cache_size: usize) -> Result<Self> {
        // The header is the contract; refuse to run against a mismatch.
        // SAFETY: no-argument query function.
        let abi = unsafe { ffi::dg_engine_abi_version() };
        assert_eq!(abi, 1, "dragoman_engine ABI version mismatch");

        let options = ffi::dg_engine_options {
            cache_size,
            log_level: ptr::null(),
        };
        let mut err: *mut ffi::dg_error = ptr::null_mut();
        // SAFETY: `options` outlives the call; `err` is a valid out-pointer.
        let raw = unsafe { ffi::dg_engine_create(&options, &mut err) };
        match NonNull::new(raw) {
            Some(ptr) => Ok(Engine {
                handle: Arc::new(EngineHandle { ptr }),
                _not_sync: PhantomData,
            }),
            None => Err(Error::Engine(take_error(err, "engine create"))),
        }
    }

    /// Loads a model. `config_yaml` is the path-free Marian configuration;
    /// see [`crate::marian_config`].
    pub fn load(&mut self, files: &ModelFiles, config_yaml: &str) -> Result<Model> {
        let model = path_to_cstring(&files.model)?;
        let vocab = files.vocab.as_deref().map(path_to_cstring).transpose()?;
        let src_vocab = files
            .src_vocab
            .as_deref()
            .map(path_to_cstring)
            .transpose()?;
        let trg_vocab = files
            .trg_vocab
            .as_deref()
            .map(path_to_cstring)
            .transpose()?;
        let shortlist = files
            .shortlist
            .as_deref()
            .map(path_to_cstring)
            .transpose()?;
        let config = CString::new(config_yaml)
            .map_err(|_| Error::InvalidInput("config contains NUL".into()))?;

        let as_ptr = |s: &Option<CString>| s.as_ref().map_or(ptr::null(), |s| s.as_ptr());
        let c_files = ffi::dg_model_files {
            model_path: model.as_ptr(),
            vocab_path: as_ptr(&vocab),
            src_vocab_path: as_ptr(&src_vocab),
            trg_vocab_path: as_ptr(&trg_vocab),
            shortlist_path: as_ptr(&shortlist),
        };

        let mut err: *mut ffi::dg_error = ptr::null_mut();
        // SAFETY: the engine pointer is live (we hold the Arc); all strings
        // in `c_files` and `config` outlive the call.
        let raw = unsafe {
            ffi::dg_model_load(
                self.handle.ptr.as_ptr(),
                &c_files,
                config.as_ptr(),
                &mut err,
            )
        };
        match NonNull::new(raw) {
            Some(ptr) => Ok(Model {
                ptr,
                engine: Arc::clone(&self.handle),
                _not_sync: PhantomData,
            }),
            None => Err(Error::Load(take_error(err, "model load"))),
        }
    }

    /// Translates `segments` with `first`, pivoting through `second` when
    /// given. Both models must have been loaded on this engine.
    pub fn translate_batch(
        &mut self,
        first: &Model,
        second: Option<&Model>,
        segments: &[String],
        options: TranslateOptions,
    ) -> Result<Vec<String>> {
        for model in std::iter::once(first).chain(second) {
            if !Arc::ptr_eq(&model.engine, &self.handle) {
                return Err(Error::InvalidInput(
                    "model was loaded on a different engine".into(),
                ));
            }
        }

        let texts: Vec<ffi::dg_text> = segments
            .iter()
            .map(|s| ffi::dg_text {
                data: s.as_ptr().cast(),
                len: s.len(),
            })
            .collect();
        let c_options = ffi::dg_translate_options {
            html: options.html.into(),
        };

        let mut out: *mut ffi::dg_result = ptr::null_mut();
        let mut err: *mut ffi::dg_error = ptr::null_mut();
        // SAFETY: engine and models are live and belong together (checked
        // above); `texts` borrows `segments`, which outlives the call; `out`
        // and `err` are valid out-pointers.
        let rc = unsafe {
            ffi::dg_translate_batch(
                self.handle.ptr.as_ptr(),
                first.ptr.as_ptr(),
                second.map_or(ptr::null_mut(), |m| m.ptr.as_ptr()),
                texts.as_ptr(),
                texts.len(),
                &c_options,
                &mut out,
                &mut err,
            )
        };
        if rc != 0 {
            return Err(Error::Engine(take_error(err, "translate")));
        }

        // Frees the result even if UTF-8 conversion below panics.
        struct ResultGuard(*mut ffi::dg_result);
        impl Drop for ResultGuard {
            fn drop(&mut self) {
                // SAFETY: the pointer was stored by a successful
                // dg_translate_batch and is freed exactly once.
                unsafe { ffi::dg_result_free(self.0) };
            }
        }
        let guard = ResultGuard(out);

        // SAFETY: `out` is a valid result from the successful call above;
        // each returned text is valid until dg_result_free.
        let translations = unsafe {
            let n = ffi::dg_result_len(guard.0);
            (0..n)
                .map(|i| {
                    let text = ffi::dg_result_text(guard.0, i);
                    let bytes = std::slice::from_raw_parts(text.data.cast::<u8>(), text.len);
                    String::from_utf8_lossy(bytes).into_owned()
                })
                .collect()
        };
        Ok(translations)
    }
}

/// The real engine as a [`Backend`].
pub struct BergamotBackend {
    engine: Engine,
}

impl BergamotBackend {
    pub fn new(cache_size: usize) -> Result<Self> {
        Ok(BergamotBackend {
            engine: Engine::new(cache_size)?,
        })
    }
}

impl Backend for BergamotBackend {
    type Model = Model;

    fn load(&mut self, files: &ModelFiles, config_yaml: &str) -> Result<Model> {
        self.engine.load(files, config_yaml)
    }

    fn translate(
        &mut self,
        first: &Model,
        second: Option<&Model>,
        segments: Vec<String>,
        options: TranslateOptions,
    ) -> Result<Vec<String>> {
        self.engine
            .translate_batch(first, second, &segments, options)
    }
}
