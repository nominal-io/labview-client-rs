# All build entry points — nobody runs raw cargo commands.
#
# Output naming is load-bearing: LabVIEW wildcard-matches on lv_src/bin/, so
# every build recipe renames Cargo's output (nominal_ffi.dll /
# libnominal_ffi.so) to the nominalClient_* convention on copy.

# On Windows, run recipes under PowerShell (no `sh` required; `cp` is a
# built-in alias for Copy-Item there, so recipes stay identical).
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# Windows 64-bit
build-win64:
    cargo build --release --target x86_64-pc-windows-msvc -p ffi
    cp target/x86_64-pc-windows-msvc/release/nominal_ffi.dll lv_src/bin/nominalClient_64.dll

# Windows 32-bit — needs NASM on PATH (winget install -e --id NASM.NASM):
# aws-lc-sys (rustls's crypto backend, via `nominal`) assembles i686 code.
build-win32:
    cargo build --release --target i686-pc-windows-msvc -p ffi
    cp target/i686-pc-windows-msvc/release/nominal_ffi.dll lv_src/bin/nominalClient_32.dll

# Linux (Ubuntu + NI Linux RT — musl, see CLAUDE.md Platform Notes)
build-linux:
    cargo build --release --target x86_64-unknown-linux-musl -p ffi
    cp target/x86_64-unknown-linux-musl/release/libnominal_ffi.so lv_src/bin/nominalClient_64.so

# macOS — best-effort only, do not chase failures here
build-macos:
    cargo build --release --target x86_64-apple-darwin -p ffi || echo "macOS build skipped/failed — not required"

build-all: build-win64 build-win32 build-linux

test:
    cargo test --workspace

lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
