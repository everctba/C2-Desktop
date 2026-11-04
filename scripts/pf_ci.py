import io
import os

os.makedirs(r".github\workflows", exist_ok=True)

ci = """name: ci

on:
  push:
    branches: [master, main]
  pull_request:

jobs:
  core:
    name: Core + CLI (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: fmt
        run: cargo fmt --all -- --check
      - name: clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: test
        run: cargo test --workspace
      - name: build release
        run: cargo build --workspace --release
      - name: smoke test (linux)
        if: runner.os == 'Linux'
        run: |
          ./target/release/c2 --version
          mkdir -p /tmp/c2-smoke && cd /tmp/c2-smoke
          printf 'fn main() {}\\n' > main.rs
          ../..${{ github.workspace }}/target/release/c2 --root . init --json
          ../../${{ github.workspace }}/target/release/c2 --root . status --json

  desktop:
    name: Desktop (windows)
    runs-on: windows-latest
    defaults:
      run:
        working-directory: apps/desktop
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: pnpm/action-setup@v4
        with:
          version: 10
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: apps/desktop/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
      - run: pnpm build
      - run: cargo build --manifest-path src-tauri/Cargo.toml
"""
io.open(r".github\workflows\ci.yml", "w", encoding="utf-8", newline="\n").write(ci)
print("ok: .github/workflows/ci.yml")
