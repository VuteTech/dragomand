# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds the vendored Mozilla translation engine (engine/vendor/) natively,
# library only, plus our C ABI adapter and the dg-smoke test program.
#
#   cmake -S engine -B engine/build -G Ninja
#   cmake --build engine/build
#
# Useful knobs:
#   -DBUILD_ARCH=x86-64-v2   target ISA (default: native; use v2 for packages)
#   -DCMAKE_BUILD_TYPE=...   default RelWithDebInfo (vendor default)

cmake_minimum_required(VERSION 3.25)

project(dragoman_engine C CXX)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Parts of the vendored tree declare pre-3.5 CMake minimums that CMake 4
# refuses without this.
set(CMAKE_POLICY_VERSION_MINIMUM 3.5)

# --- Vendored engine configuration -----------------------------------------
# Library-only native build. USE_MKL must be OFF: MKL is proprietary and
# GPL-incompatible (docs/packaging.md "BLAS"). Marian's FindCBLAS is replaced by our
# pkg-config version (engine/patches/), which fails hard when no CBLAS exists.
set(COMPILE_WASM OFF CACHE BOOL "" FORCE)
set(COMPILE_CUDA OFF CACHE BOOL "" FORCE)
set(COMPILE_CPU ON CACHE BOOL "" FORCE)
set(USE_MKL OFF CACHE BOOL "" FORCE)
set(USE_FBGEMM OFF CACHE BOOL "" FORCE)
# OFF so system libraries (BLAS above all) link dynamically: marian's
# USE_STATIC_LIBS restricts find_library to .a while searching for BLAS, and
# distributions ship BLAS shared. The engine itself is a static library
# regardless of this switch.
set(USE_STATIC_LIBS OFF CACHE BOOL "" FORCE)
set(USE_SENTENCEPIECE ON CACHE BOOL "" FORCE)
set(COMPILE_LIBRARY_ONLY ON CACHE BOOL "" FORCE)
set(COMPILE_TESTS OFF CACHE BOOL "" FORCE)
set(COMPILE_UNIT_TESTS OFF CACHE BOOL "" FORCE)
set(COMPILE_EXAMPLES OFF CACHE BOOL "" FORCE)
set(COMPILE_SERVER OFF CACHE BOOL "" FORCE)
set(USE_DOXYGEN OFF CACHE BOOL "" FORCE)
set(USE_CCACHE OFF CACHE BOOL "" FORCE)
set(GENERATE_MARIAN_INSTALL_TARGETS OFF CACHE BOOL "" FORCE)
set(SSPLIT_COMPILE_LIBRARY_ONLY ON CACHE BOOL "" FORCE)
# sentencepiece would link tcmalloc_minimal when present, silently replacing
# the process allocator (including the Rust daemon's) and adding a gperftools
# runtime dependency. Keep the default allocator.
set(SPM_ENABLE_TCMALLOC OFF CACHE BOOL "" FORCE)

add_subdirectory(vendor)

if(USE_MKL)
  message(FATAL_ERROR "USE_MKL must stay OFF: MKL is proprietary and GPL-incompatible.")
endif()

# marian's bundled faiss calls a few LAPACK routines (dsyev_, sgeqrf_,
# sorgqr_); MKL used to provide them implicitly. Resolve LAPACK the same way
# as CBLAS: through pkg-config, never MKL.
find_package(PkgConfig REQUIRED)
pkg_search_module(DG_LAPACK REQUIRED lapack openblas flexiblas)

# --- Adapter ----------------------------------------------------------------
add_library(dragoman_engine STATIC adapter/dragoman_engine.cpp)
target_include_directories(dragoman_engine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/adapter)
target_link_libraries(dragoman_engine PUBLIC bergamot-translator-source ${DG_LAPACK_LINK_LIBRARIES})
set_target_properties(dragoman_engine PROPERTIES CXX_STANDARD 17 CXX_STANDARD_REQUIRED ON)

# --- Smoke test -------------------------------------------------------------
add_executable(dg-smoke smoke/smoke.c)
target_link_libraries(dg-smoke PRIVATE dragoman_engine)
# smoke.c is C, but everything under it is C++.
set_target_properties(dg-smoke PROPERTIES LINKER_LANGUAGE CXX)
