#!/usr/bin/env bash
# 交叉编译所有目标平台的 .node 到 npm/core/native/。
#
# 依赖：
#   - rustup（安装各交叉目标的 rust-std）
#   - zig（linux 目标的交叉链接器，cargo-zigbuild 后端）
#
# macOS 目标由 clang 原生支持多架构，无需 zig；
# linux 目标由 cargo-zigbuild 用 zig cc 交叉链接。
set -euo pipefail

# 定位 zig（优先 PATH，其次 ~/zig）
if ! command -v zig >/dev/null 2>&1; then
  if [ -x "$HOME/zig/zig" ]; then
    export PATH="$HOME/zig:$PATH"
  else
    echo "错误：未找到 zig。linux 目标交叉链接需要 zig。" >&2
    echo "  安装：curl -sL https://ziglang.org/download/0.15.2/zig-aarch64-macos-0.15.2.tar.xz | tar -xJ -C \"\$HOME/zig\" --strip-components=1" >&2
    exit 1
  fi
fi

cd "$(dirname "$0")/../npm/core"

# 交叉目标的 rust-std（本机 aarch64-apple-darwin 已默认安装）
rustup target add \
  x86_64-apple-darwin \
  x86_64-unknown-linux-gnu \
  aarch64-unknown-linux-gnu \
  x86_64-unknown-linux-musl \
  aarch64-unknown-linux-musl

NAPI_ARGS="--platform --release --manifest-path ../../crates/sift-node/Cargo.toml --output-dir native"

# macOS：本机 arm64 + 交叉 x64
npx napi build $NAPI_ARGS --target aarch64-apple-darwin
npx napi build $NAPI_ARGS --target x86_64-apple-darwin

# Linux：cargo-zigbuild 交叉链接
npx napi build $NAPI_ARGS --cross-compile --target x86_64-unknown-linux-gnu
npx napi build $NAPI_ARGS --cross-compile --target aarch64-unknown-linux-gnu
npx napi build $NAPI_ARGS --cross-compile --target x86_64-unknown-linux-musl
npx napi build $NAPI_ARGS --cross-compile --target aarch64-unknown-linux-musl

# napi 每次会生成空的 index.d.ts，清理掉
rm -f native/index.d.ts

echo "=== 产物 ==="
ls -la native/*.node
