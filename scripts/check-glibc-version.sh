#!/usr/bin/env bash
# 检查 ELF 动态符号要求的最高 glibc 版本没有超过指定基线。
set -euo pipefail

binary="${1:?用法: check-glibc-version.sh <ELF 文件> [最高 glibc 版本]}"
maximum="${2:-2.28}"

if ! command -v readelf >/dev/null 2>&1; then
  echo "错误：需要 readelf 才能检查 glibc 符号版本。" >&2
  exit 1
fi

required="$({
  LC_ALL=C readelf -W --version-info --dyn-syms "$binary" \
    | sed -nE 's/.*GLIBC_([0-9]+\.[0-9]+).*/\1/p'
  echo 0
} | sort -V | tail -n 1)"

if [ "$(printf '%s\n%s\n' "$required" "$maximum" | sort -V | tail -n 1)" != "$maximum" ]; then
  echo "错误：$binary 要求 GLIBC_$required，超过允许的 GLIBC_$maximum。" >&2
  exit 1
fi

echo "glibc ABI 检查通过：$binary 最高要求 GLIBC_$required（允许 <= GLIBC_$maximum）"
