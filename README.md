> [!WARNING]  
> This plugin compiles and *should* run, but it requires WASI threads (basically multi-threaded wasm). 
> Wazero has only experimental support for wasi-threads.
> As a result, plugin crashes right now with error `instantiating module: module[env] not instantiated`

# Bliss-rs Audio Analysis plugin

A Navidrome plugin written in Rust that does performs analysis with bliss-rs. 
It periodically (default 24h) analyzes audio files about in all configured music libraries stores analysis data in it's database.
Files that have been analyzed previously are skipped (no way to force a re-analysis for now).

## Features


## Configuration

Configure in the Navidrome UI (Settings → Plugins → navidrome-blissrs):

| Key               | Description                                                              |    Default |
|-------------------|--------------------------------------------------------------------------|------------|
|`schedule`         | When the plugin runs the analysis. Don't set frequency too low           |`@every 10m`|
|`file_limit`       | Limit plugin to only analyze X files per-library per-run (0 = disabled)  |        100 |
|`ignored_libraries`| Library IDs to ignore - comma-seperated (e.g. "2,4")                     |      empty |


## Requirements
- Rust toolchain with wasm32-wasip1-threads target
```bash
# Install the WASM target if you haven't already
rustup target add wasm32-wasip1-threads
```
- C Wasi SDK to cross-compile aubio https://github.com/aubio/aubio (or use libaubio.a from here)
```bash
# below is for systems using debian package manager, other systems may vary.
wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-29/wasi-sdk-29.0-arm64-linux.deb
dpgk -i ./wasi-sdk-29.0-arm64-linux.deb
```
- Navidrome with plugins enabled

## Building

```bash
git clone https://github.com/metalheim/navidrome-blissrs
cd navidrome-blissrs

# Compile aubio
git clone https://github.com/aubio/aubio
cd aubio
cat > wasi.toolchain.cmake << EOF
set(CMAKE_SYSTEM_NAME WASI)
set(CMAKE_SYSTEM_VERSION 1)
set(CMAKE_C_COMPILER "/opt/wasi-sdk/bin/clang")
set(CMAKE_AR "/opt/wasi-sdk/bin/llvm-ar")
set(CMAKE_RANLIB "/opt/wasi-sdk/bin/llvm-ranlib")
set(CMAKE_SYSROOT "/opt/wasi-sdk/share/wasi-sysroot")
set(CMAKE_C_FLAGS "--sysroot=/opt/wasi-sdk/share/wasi-sysroot" CACHE STRING "" FORCE)
set(CMAKE_EXE_LINKER_FLAGS "--sysroot=/opt/wasi-sdk/share/wasi-sysroot" CACHE STRING "" FORCE)
EOF
mkdir build-wasi && cd build-wasi
cmake -DCMAKE_TOOLCHAIN_FILE=../wasi.toolchain.cmake ..   -DBUILD_SHARED_LIBS=OFF   -Denable-tests=OFF   -Denable-examples=OFF   -Denable-avcodec=OFF   -Denable-sndfile=OFF   -Denable-samplerate=OFF   -Denable-rubberband=OFF   -Denable-fftw3=OFF   -Denable-vorbis=OFF   -Denable-flac=OFF
make

export CC=/opt/wasi-sdk/bin/clang
export AR=/opt/wasi-sdk/bin/ar
export CFLAGS="--sysroot=/opt/wasi-sdk/share/wasi-sysroot"
export LIBRARY_PATH="/your/full/path/to/navidrome-blissrs/aubio/build-wasi/src":$LIBRARY_PATH
export RUSTFLAGS="-L native=/your/full/path/to/navidrome-blissrs/aubio/build-wasi/src"

# Build the plugin (this calls cargo build and packages the wasm file+manifest into an ndp file
./build-ndp.sh

```