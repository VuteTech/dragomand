set(SGEMM_SOURCES
  matmul.cpp
  utils/broadcast_utils.cpp
  utils/shape_utils.cpp)

if(ONNXJS_SGEMM)
  list(APPEND SGEMM_SOURCES gemm.cpp)
endif(ONNXJS_SGEMM)

add_library(onnx-sgemm STATIC ${SGEMM_SOURCES})
target_include_directories(onnx-sgemm PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

if(ONNXJS_SGEMM)
  target_link_libraries(onnx-sgemm eigen)
endif(ONNXJS_SGEMM)
