// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
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

    /// Written by engine/CMakeLists.txt: everything the static engine
    /// depends on, in link order. It differs per distribution (which BLAS,
    /// static or shared PCRE2) and per architecture (intgemm or ruy).
    const LINK_LIST: &str = "engine-link.txt";

    pub fn link() {
        println!("cargo:rerun-if-env-changed=DRAGOMAN_ENGINE_LIB_DIR");

        let build_dir = match env::var_os("DRAGOMAN_ENGINE_LIB_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => build_with_cmake(),
        };

        let list_path = build_dir.join(LINK_LIST);
        let list = std::fs::read_to_string(&list_path).unwrap_or_else(|e| {
            panic!(
                "{}: {e}; is the engine built and configured by this tree's \
                 engine/CMakeLists.txt? (cmake -S engine -B engine/build && \
                 cmake --build engine/build)",
                list_path.display()
            )
        });
        println!("cargo:rerun-if-changed={}", list_path.display());

        for item in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if let Some(name) = item.strip_prefix("-l") {
                println!("cargo:rustc-link-lib=dylib={name}");
                continue;
            }
            let path = Path::new(item);
            let (dir, kind, name) = split_library(path)
                .unwrap_or_else(|| panic!("{}: cannot link {item:?}", list_path.display()));
            assert!(
                path.exists(),
                "{} not found; is the engine built? (cmake --build engine/build)",
                path.display()
            );
            if kind == "static" {
                // Relink when a prebuilt archive changes underneath us.
                println!("cargo:rerun-if-changed={}", path.display());
            }
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib={kind}={name}");
        }
        // The engine is C++; the C++ linker driver would add this itself.
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    /// `/usr/lib/libfoo.so.3` becomes (`/usr/lib`, "dylib", "foo"),
    /// `build/libbar.a` becomes (`build`, "static", "bar").
    fn split_library(path: &Path) -> Option<(&Path, &'static str, &str)> {
        let dir = path.parent()?;
        let file = path.file_name()?.to_str()?.strip_prefix("lib")?;
        if let Some(name) = file.strip_suffix(".a") {
            return Some((dir, "static", name));
        }
        let (name, _) = file.split_once(".so")?;
        Some((dir, "dylib", name))
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
