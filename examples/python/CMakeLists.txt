cmake_minimum_required(VERSION 3.15...3.30)
project(steam_ffi_python LANGUAGES CXX)

find_package(Python 3.10 COMPONENTS Interpreter Development.Module REQUIRED)
find_package(nanobind CONFIG REQUIRED)

if(NOT DEFINED STEAM_FFI_LIB_DIR)
    message(FATAL_ERROR "STEAM_FFI_LIB_DIR must be set to the directory containing libsteam_ffi")
endif()

if(NOT DEFINED STEAM_FFI_BINDINGS_DIR)
    message(FATAL_ERROR "STEAM_FFI_BINDINGS_DIR must be set to the diplomat cpp bindings directory")
endif()

nanobind_add_module(steam_ffi_ext STABLE_ABI NB_STATIC src/bindings.cpp)

target_include_directories(steam_ffi_ext PRIVATE ${STEAM_FFI_BINDINGS_DIR})

if(WIN32)
    target_link_libraries(steam_ffi_ext PRIVATE ${STEAM_FFI_LIB_DIR}/steam_ffi.dll.lib)
else()
    target_link_libraries(steam_ffi_ext PRIVATE ${STEAM_FFI_LIB_DIR}/libsteam_ffi${CMAKE_SHARED_LIBRARY_SUFFIX})
endif()

install(TARGETS steam_ffi_ext LIBRARY DESTINATION .)
