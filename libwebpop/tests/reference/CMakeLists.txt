cmake_minimum_required(VERSION 3.16)
project(webpop_oracle C)

# Keep this configuration explicit: the native oracle must always be scalar.
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(WEBP_ENABLE_SIMD OFF CACHE BOOL "" FORCE)
set(WEBP_USE_THREAD OFF CACHE BOOL "" FORCE)
foreach(option ANIM_UTILS CWEBP DWEBP GIF2WEBP IMG2WEBP VWEBP WEBPINFO
               LIBWEBPMUX WEBPMUX EXTRAS WEBP_JS FUZZTEST)
  set(WEBP_BUILD_${option} OFF CACHE BOOL "" FORCE)
endforeach()
set(WEBP_NEAR_LOSSLESS ON CACHE BOOL "" FORCE)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
get_filename_component(UPSTREAM "${CMAKE_CURRENT_SOURCE_DIR}/../../../upstream" ABSOLUTE)
add_subdirectory("${UPSTREAM}" upstream EXCLUDE_FROM_ALL)
add_executable(webpop_oracle oracle.c huffman.c references.c histogram.c)
target_include_directories(webpop_oracle PRIVATE "${UPSTREAM}" "${CMAKE_CURRENT_BINARY_DIR}/upstream")
target_compile_definitions(webpop_oracle PRIVATE HAVE_CONFIG_H)
target_link_libraries(webpop_oracle PRIVATE webp)
set_target_properties(webpop_oracle PROPERTIES C_STANDARD 99 C_STANDARD_REQUIRED YES)
target_sources(webpop_oracle PRIVATE near_lossless.c)

target_sources(webpop_oracle PRIVATE predictor.c)
