# All build entry points — nobody runs raw cargo commands.
#
# Output naming is load-bearing: LabVIEW wildcard-matches on lv_src/bin/, so
# every build recipe renames Cargo's output (nominal_ffi.dll /
# libnominal_ffi.so) to the nominalClient_* convention on copy.

# On Windows, run recipes under PowerShell (no `sh` required; `cp` is a
# built-in alias for Copy-Item there, so recipes stay identical).
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# Regenerate the C header. Uses the cbindgen CLI (`cargo install cbindgen`),
# NOT a build-dependency — cbindgen is MPL-2.0, which org policy prohibits in
# the dependency tree. Every build recipe depends on this, and CI fails if
# the committed header is stale (see ci.yml).
header:
    cbindgen --config crates/ffi/cbindgen.toml --output crates/ffi/include/nominal_ffi.h crates/ffi
    cp crates/ffi/include/nominal_ffi.h lv_src/bin/nominal_ffi.h

# Windows 64-bit
build-win64: header
    cargo build --release --target x86_64-pc-windows-msvc -p ffi
    cp target/x86_64-pc-windows-msvc/release/nominal_ffi.dll lv_src/bin/nominalClient_64.dll

# Windows 32-bit — needs NASM on PATH (winget install -e --id NASM.NASM):
# aws-lc-sys (rustls's crypto backend, via `nominal`) assembles i686 code.
build-win32: header
    cargo build --release --target i686-pc-windows-msvc -p ffi
    cp target/i686-pc-windows-msvc/release/nominal_ffi.dll lv_src/bin/nominalClient_32.dll

# Linux (Ubuntu + NI Linux RT). gnu, not musl: rustc cannot build a cdylib
# for musl, and a shared object loaded by glibc-linked LabVIEW must use the
# system's dynamic linker anyway. Build on an old glibc baseline for reach —
# CI pins ubuntu-22.04 (glibc 2.35). See CLAUDE.md Platform Notes.
build-linux: header
    cargo build --release --target x86_64-unknown-linux-gnu -p ffi
    cp target/x86_64-unknown-linux-gnu/release/libnominal_ffi.so lv_src/bin/nominalClient_64.so

# macOS — best-effort only, do not chase failures here
build-macos:
    cargo build --release --target x86_64-apple-darwin -p ffi || echo "macOS build skipped/failed — not required"

build-all: build-win64 build-win32 build-linux

test:
    cargo test --workspace

lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
