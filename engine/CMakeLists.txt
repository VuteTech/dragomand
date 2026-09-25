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

# On ARM, marian compiles its own sources with directory-level definitions
# (ARM, FMA, SSE, USE_RUY_SGEMM, ...) and options (-flax-vector-conversions)
# that select the NEON code in the headers it shares with bergamot. The
# bergamot sources and our adapter include those headers from outside
# marian's directory, so they would see the x86 half of simd_utils and fail
# (missing <fenv.h>, sin_ps). Give them marian's settings; PUBLIC carries
# them on to the adapter. x86-64 builds need none of this and are left as
# they are.
if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64|ARM64|arm.*)$")
  get_directory_property(_dg_marian_defs DIRECTORY vendor/marian-fork COMPILE_DEFINITIONS)
  get_directory_property(_dg_marian_opts DIRECTORY vendor/marian-fork COMPILE_OPTIONS)
  message(STATUS "ARM: applying marian's definitions to bergamot and the adapter: ${_dg_marian_defs}")
  target_compile_definitions(bergamot-translator-source PUBLIC ${_dg_marian_defs})
  target_compile_options(bergamot-translator-source PUBLIC ${_dg_marian_opts})
endif()

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

# --- Link list for the Rust side --------------------------------------------
# The daemon links this static library from Rust
# (crates/dragoman-engine/build.rs), so everything the engine depends on must
# reach that link too, and it differs per distribution (which BLAS, a static
# or shared PCRE2) and per architecture (intgemm on x86-64, ruy on ARM64).
# Walk the dependency graph CMake already knows and write it out, one item
# per line in link order: archive and library paths, or -l flags.
function(_dg_link_walk item depth)
  if(depth GREATER 20)
    message(FATAL_ERROR "link graph too deep at ${item}")
  endif()
  string(REGEX REPLACE "^\\$<LINK_ONLY:(.*)>$" "\\1" item "${item}")
  if(TARGET ${item})
    get_target_property(aliased ${item} ALIASED_TARGET)
    if(aliased)
      set(item ${aliased})
    endif()
  endif()
  # Every library goes after everything that uses it: drop an earlier
  # occurrence and append.
  get_property(items GLOBAL PROPERTY DG_LINK_ITEMS)
  list(REMOVE_ITEM items "${item}")
  list(APPEND items "${item}")
  set_property(GLOBAL PROPERTY DG_LINK_ITEMS "${items}")
  if(TARGET ${item})
    get_target_property(type ${item} TYPE)
    get_target_property(imported ${item} IMPORTED)
    set(deps "")
    if(NOT imported AND NOT type STREQUAL "INTERFACE_LIBRARY")
      get_target_property(private ${item} LINK_LIBRARIES)
      if(private)
        list(APPEND deps ${private})
      endif()
    endif()
    get_target_property(public ${item} INTERFACE_LINK_LIBRARIES)
    if(public)
      list(APPEND deps ${public})
    endif()
    list(REMOVE_DUPLICATES deps)
    foreach(dep IN LISTS deps)
      math(EXPR next "${depth} + 1")
      _dg_link_walk("${dep}" ${next})
    endforeach()
  endif()
endfunction()

set_property(GLOBAL PROPERTY DG_LINK_ITEMS "")
_dg_link_walk(dragoman_engine 0)
get_property(_dg_items GLOBAL PROPERTY DG_LINK_ITEMS)
set(_dg_lines "")
foreach(item IN LISTS _dg_items)
  if(TARGET ${item})
    get_target_property(type ${item} TYPE)
    get_target_property(imported ${item} IMPORTED)
    if(type STREQUAL "STATIC_LIBRARY" OR type STREQUAL "SHARED_LIBRARY"
       OR (imported AND type STREQUAL "UNKNOWN_LIBRARY"))
      string(APPEND _dg_lines "$<TARGET_FILE:${item}>\n")
    endif()
    # Interface and location-less imported targets (Threads::Threads)
    # contribute only through the dependencies walked above.
  elseif(item MATCHES "::")
    # A namespaced imported target out of this directory's scope
    # (Threads::Threads); it only carries flags like -pthread.
  elseif(IS_ABSOLUTE "${item}")
    string(APPEND _dg_lines "${item}\n")
  elseif(item MATCHES "^-l")
    string(APPEND _dg_lines "${item}\n")
  elseif(NOT item MATCHES "^-")
    string(APPEND _dg_lines "-l${item}\n")
  endif()
  # Other flags (-pthread) are covered by the Rust toolchain's own link.
endforeach()
file(GENERATE OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/engine-link.txt CONTENT "${_dg_lines}")

# --- Smoke test -------------------------------------------------------------
add_executable(dg-smoke smoke/smoke.c)
target_link_libraries(dg-smoke PRIVATE dragoman_engine)
# smoke.c is C, but everything under it is C++.
set_target_properties(dg-smoke PROPERTIES LINKER_LANGUAGE CXX)
