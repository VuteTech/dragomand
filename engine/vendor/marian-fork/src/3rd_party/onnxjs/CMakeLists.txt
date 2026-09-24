cmake_minimum_required(VERSION 3.5.1)

if (POLICY CMP0074)
  cmake_policy(SET CMP0074 NEW) # CMake 3.12
endif ()

project(onnx CXX C)
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Custom CMake options
# This option enables/disables the sgemm() routine in the library.
# If it is off then library provides only matrix multplication routine.
option(ONNXJS_SGEMM "Enable/disable sgemm routine support in ONNXJS" ON)

add_subdirectory(src)
if(ONNXJS_SGEMM)
  # Eigen is a dependency for enabling sgemm support
  add_subdirectory(deps/eigen)
endif(ONNXJS_SGEMM)
