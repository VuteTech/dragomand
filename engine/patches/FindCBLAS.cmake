# SPDX-License-Identifier: GPL-3.0-or-later
#
# dragomand replacement for marian-fork's cmake/FindCBLAS.cmake, installed
# over it by engine/scripts/update-vendor.sh.
#
# The original guesses library and header locations by hand and misses, for
# example, OpenBLAS headers in /usr/include/openblas (Arch), silently falling
# back to whatever else it finds. Resolve the generic CBLAS interface through
# pkg-config instead. The default order prefers `cblas` — the name behind
# which distributions put their chosen BLAS provider (netlib by default,
# OpenBLAS via Arch's blas-openblas or Debian's alternatives, FlexiBLAS on
# Fedora) — then falls back to `openblas` and `flexiblas` directly, and
# last to the generic `blas` module when it carries the CBLAS interface:
# Debian and Ubuntu ship no cblas.pc, but their `blas` (the libblas.so.3
# alternative, netlib or OpenBLAS) exports the cblas_* symbols.
# Set DG_CBLAS_MODULE to a pkg-config module name to force one (used by the
# benchmarks to compare implementations).
#
# Sets CBLAS_FOUND, CBLAS_LIBRARIES and CBLAS_INCLUDE_DIR as the original did.

find_package(PkgConfig REQUIRED)
include(CheckSymbolExists)
include(CMakePushCheckState)

if(DG_CBLAS_MODULE)
  pkg_check_modules(DG_CBLAS REQUIRED ${DG_CBLAS_MODULE})
else()
  pkg_search_module(DG_CBLAS cblas openblas flexiblas)
  if(NOT DG_CBLAS_FOUND)
    # Elsewhere `blas` can be the Fortran interface alone (Arch's netlib
    # blas has no cblas_* symbols), so only accept it when CBLAS links.
    pkg_check_modules(DG_BLAS_GENERIC QUIET blas)
    if(DG_BLAS_GENERIC_FOUND)
      cmake_push_check_state(RESET)
      set(CMAKE_REQUIRED_LIBRARIES ${DG_BLAS_GENERIC_LINK_LIBRARIES})
      set(CMAKE_REQUIRED_INCLUDES ${DG_BLAS_GENERIC_INCLUDE_DIRS})
      set(CMAKE_REQUIRED_QUIET TRUE)
      check_symbol_exists(cblas_sgemm "cblas.h" DG_BLAS_GENERIC_HAS_CBLAS)
      cmake_pop_check_state()
      if(DG_BLAS_GENERIC_HAS_CBLAS)
        set(DG_CBLAS_FOUND TRUE)
        set(DG_CBLAS_MODULE_NAME blas)
        set(DG_CBLAS_LINK_LIBRARIES ${DG_BLAS_GENERIC_LINK_LIBRARIES})
        set(DG_CBLAS_INCLUDE_DIRS ${DG_BLAS_GENERIC_INCLUDE_DIRS})
      endif()
    endif()
  endif()
endif()

if(DG_CBLAS_FOUND)
  set(CBLAS_FOUND TRUE)
  set(CBLAS_LIBRARIES ${DG_CBLAS_LINK_LIBRARIES})
  set(CBLAS_INCLUDE_DIR ${DG_CBLAS_INCLUDE_DIRS})
  if(NOT CBLAS_INCLUDE_DIR)
    # pkg-config reports nothing for default include paths.
    set(CBLAS_INCLUDE_DIR /usr/include)
  endif()
  message(STATUS "Found CBLAS via pkg-config (${DG_CBLAS_MODULE_NAME}): "
                 "${CBLAS_LIBRARIES}; headers in ${CBLAS_INCLUDE_DIR}")
else()
  set(CBLAS_FOUND FALSE)
  message(FATAL_ERROR
    "No CBLAS found through pkg-config (tried: cblas, openblas, flexiblas, "
    "and blas with the CBLAS interface). "
    "Install a CBLAS implementation; OpenBLAS is recommended.")
endif()
