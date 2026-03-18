cmake_minimum_required(VERSION 3.16)
project(nam_wrapper CXX C)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED OFF)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

if (CMAKE_SYSTEM_NAME STREQUAL "Darwin")
  set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -stdlib=libc++")
endif()

set(NAM_CORE_DIR ${CMAKE_SOURCE_DIR}/../NeuralAmpModelerCore)
set(NAM_DEPS_DIR ${NAM_CORE_DIR}/Dependencies)

file(GLOB_RECURSE NAM_SOURCES
  ${NAM_CORE_DIR}/NAM/*.cpp
  ${NAM_CORE_DIR}/NAM/*.c
)

set(AUDIO_DSP_TOOLS_SOURCES
  ${NAM_DEPS_DIR}/AudioDSPTools/dsp/dsp.cpp
  ${NAM_DEPS_DIR}/AudioDSPTools/dsp/wav.cpp
  ${NAM_DEPS_DIR}/AudioDSPTools/dsp/ImpulseResponse.cpp
  ${NAM_DEPS_DIR}/AudioDSPTools/dsp/NoiseGate.cpp
  ${NAM_DEPS_DIR}/AudioDSPTools/dsp/RecursiveLinearFilter.cpp
)

add_library(nam_wrapper SHARED
  nam_wrapper.cpp
  nam_tone_stack.cpp
  ${NAM_SOURCES}
  ${AUDIO_DSP_TOOLS_SOURCES}
)

target_include_directories(nam_wrapper PUBLIC
  ${CMAKE_SOURCE_DIR}
)

target_include_directories(nam_wrapper PRIVATE
  ${NAM_CORE_DIR}
  ${NAM_DEPS_DIR}/eigen
  ${NAM_DEPS_DIR}/nlohmann
  ${NAM_DEPS_DIR}/AudioDSPTools
  ${NAM_DEPS_DIR}/AudioDSPTools/Dependencies/eigen
  ${NAM_DEPS_DIR}/AudioDSPTools/Dependencies/nlohmann
)

target_compile_definitions(nam_wrapper PRIVATE
  EIGEN_MAX_ALIGN_BYTES=0
  EIGEN_DONT_VECTORIZE
)

if (CMAKE_SYSTEM_NAME STREQUAL "Darwin")
  target_compile_options(nam_wrapper PRIVATE -Wall -Wextra -Wpedantic -Wno-unused-parameter)
else()
  target_compile_options(nam_wrapper PRIVATE -Wall -Wextra -Wpedantic -Wno-unused-parameter)
endif()

# Igual ao projeto original: esses arquivos podem gerar warnings chatos
set_source_files_properties(${NAM_CORE_DIR}/NAM/dsp.cpp PROPERTIES COMPILE_FLAGS "-Wno-error")
set_source_files_properties(${NAM_CORE_DIR}/NAM/conv1d.cpp PROPERTIES COMPILE_FLAGS "-Wno-error")

install(TARGETS nam_wrapper
  LIBRARY DESTINATION lib
  ARCHIVE DESTINATION lib
  RUNTIME DESTINATION lib
)

install(FILES nam_wrapper.h DESTINATION include)
