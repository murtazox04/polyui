cmake_minimum_required(VERSION 3.22.1)
project(polyui_jni C)

if(NOT POLYUI_REPO_ROOT)
    message(FATAL_ERROR "Pass -DPOLYUI_REPO_ROOT=<path to the polyui repository>")
endif()

if(NOT POLYUI_CARGO_TARGET_DIR)
    set(POLYUI_CARGO_TARGET_DIR "${POLYUI_REPO_ROOT}/target")
endif()

set(POLYUI_TRIPLE_arm64-v8a aarch64-linux-android)
set(POLYUI_TRIPLE_armeabi-v7a armv7-linux-androideabi)
set(POLYUI_TRIPLE_x86_64 x86_64-linux-android)
set(POLYUI_TRIPLE ${POLYUI_TRIPLE_${ANDROID_ABI}})

if(NOT POLYUI_TRIPLE)
    message(FATAL_ERROR "PolyUI has no Rust target for the ${ANDROID_ABI} ABI")
endif()

add_library(polyui_ffi STATIC IMPORTED)
set_target_properties(polyui_ffi PROPERTIES
    IMPORTED_LOCATION "${POLYUI_CARGO_TARGET_DIR}/${POLYUI_TRIPLE}/release/libpolyui_ffi.a")

add_library(polyui_jni SHARED polyui_jni.c)
set_target_properties(polyui_jni PROPERTIES C_STANDARD 11 C_VISIBILITY_PRESET hidden)
target_include_directories(polyui_jni PRIVATE "${POLYUI_REPO_ROOT}/bindings/ffi/include")
target_compile_options(polyui_jni PRIVATE -Wall -Wextra -Werror -ffunction-sections -fdata-sections)
target_link_options(polyui_jni PRIVATE
    -Wl,--gc-sections
    -Wl,--exclude-libs,ALL
    -Wl,-z,max-page-size=16384)
target_link_libraries(polyui_jni PRIVATE polyui_ffi log dl m)
