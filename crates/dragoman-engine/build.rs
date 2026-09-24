// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds and links the C++ engine (engine/CMakeLists.txt) when the
//! `bergamot` feature is on.
//!
//! `DRAGOMAN_ENGINE_LIB_DIR` can point at an existing CMake build
//! directory (for example `engine/build`) to skip the long C++ build in
//! development and CI; otherwise the engine is built into OUT_DIR.

fn main() {
    #[cfg(feature = "bergamot")]
    bergamot::link();
}

#[cfg(feature = "bergamot")]
mod bergamot {
    use std::env;
    use std::path::{Path, PathBuf};

    /// Static libraries produced by the engine build, as
    /// (subdirectory inside the build tree, library name),
    /// in link order: our adapter first, foundations last.
    const STATIC_LIBS: &[(&str, &str)] = &[
        ("", "dragoman_engine"),
        ("vendor/src/translator", "bergamot-translator-source"),
        ("", "marian"),
        ("", "ssplit"),
        ("vendor/marian-fork/src/3rd_party/intgemm", "intgemm"),
        (
            "vendor/marian-fork/src/3rd_party/sentencepiece/src",
            "sentencepiece_train",
        ),
        (
            "vendor/marian-fork/src/3rd_party/sentencepiece/src",
            "sentencepiece",
        ),
    ];

    /// System libraries the engine expects at runtime. BLAS names follow the
    /// generic CBLAS interface (docs/packaging.md "BLAS"); pcre2 comes from ssplit.
    const DYLIBS: &[&str] = &["cblas", "blas", "lapack", "pcre2-8", "stdc++"];

    pub fn link() {
        println!("cargo:rerun-if-env-changed=DRAGOMAN_ENGINE_LIB_DIR");

        let build_dir = match env::var_os("DRAGOMAN_ENGINE_LIB_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => build_with_cmake(),
        };

        for (subdir, lib) in STATIC_LIBS {
            let dir = build_dir.join(subdir);
            let archive = dir.join(format!("lib{lib}.a"));
            assert!(
                archive.exists(),
                "lib{lib}.a not found in {}; is the engine built? \
                 (cmake -S engine -B engine/build && cmake --build engine/build)",
                dir.display()
            );
            // Relink when a prebuilt archive changes underneath us.
            println!("cargo:rerun-if-changed={}", archive.display());
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static={lib}");
        }
        for lib in DYLIBS {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }

    fn build_with_cmake() -> PathBuf {
        let engine_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../engine");
        println!(
            "cargo:rerun-if-changed={}",
            engine_dir.join("adapter").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            engine_dir.join("CMakeLists.txt").display()
        );
        // Building only the adapter target skips dg-smoke and, more
        // importantly, the install step our CMakeLists does not define.
        let dst = cmake::Config::new(&engine_dir)
            .build_target("dragoman_engine")
            .build();
        dst.join("build")
    }
}
