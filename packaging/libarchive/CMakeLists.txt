cmake_minimum_required(VERSION 3.22)
project(portrait_manager_libarchive LANGUAGES C)

include(FetchContent)

# CMP0135 appeared after the Ubuntu 22.04 CMake baseline.  Newer CMake
# versions need it set explicitly when extracting a pinned URL dependency.
if(POLICY CMP0135)
  cmake_policy(SET CMP0135 NEW)
endif()

set(CMAKE_POSITION_INDEPENDENT_CODE ON CACHE BOOL "Build PIC objects for the Tauri cdylib" FORCE)
set(BUILD_SHARED_LIBS OFF CACHE BOOL "Build a static libarchive" FORCE)
set(ENABLE_TEST OFF CACHE BOOL "Do not build libarchive tests" FORCE)
set(ENABLE_TAR OFF CACHE BOOL "Do not build bsdtar" FORCE)
set(ENABLE_CPIO OFF CACHE BOOL "Do not build bsdcpio" FORCE)
set(ENABLE_CAT OFF CACHE BOOL "Do not build bsdcat" FORCE)
set(ENABLE_UNZIP OFF CACHE BOOL "Do not build bsdunzip" FORCE)
set(ENABLE_WERROR OFF CACHE BOOL "Do not inherit upstream warning policy" FORCE)
set(ENABLE_ZLIB ON CACHE BOOL "Keep ZIP deflate support" FORCE)
set(ENABLE_BZip2 ON CACHE BOOL "Keep bzip2 support" FORCE)
set(ENABLE_LZMA ON CACHE BOOL "Keep 7z LZMA/LZMA2 support" FORCE)
set(ENABLE_ZSTD ON CACHE BOOL "Keep zstd support" FORCE)
set(ENABLE_OPENSSL ON CACHE BOOL "Keep RAR crypto support" FORCE)
set(ENABLE_LIBB2 OFF CACHE BOOL "Use libarchive's documented internal BLAKE2 source" FORCE)
set(ENABLE_LZ4 OFF CACHE BOOL "Do not vary optional codec support by host" FORCE)
set(ENABLE_LZO OFF CACHE BOOL "Do not vary optional codec support by host" FORCE)
set(ENABLE_LIBXML2 OFF CACHE BOOL "Do not vary optional XML support by host" FORCE)
set(ENABLE_EXPAT OFF CACHE BOOL "Do not vary optional XML support by host" FORCE)
set(ENABLE_ACL OFF CACHE BOOL "Do not vary ACL support by host" FORCE)
set(ENABLE_XATTR OFF CACHE BOOL "Do not vary xattr support by host" FORCE)
set(ENABLE_ICONV OFF CACHE BOOL "Do not vary iconv support by host" FORCE)

FetchContent_Declare(
  libarchive
  URL https://github.com/libarchive/libarchive/releases/download/v3.8.9/libarchive-3.8.9.tar.xz
  URL_HASH SHA256=888c934f9d95648ecb9163dc8e23ab80a476ecb81a8f1154704a227b5b676dde
)
FetchContent_MakeAvailable(libarchive)
