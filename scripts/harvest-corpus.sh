#!/usr/bin/env bash
# Populate corpus/ with real-world Bazel files.
#
# Sources are shallow-cloned into corpus/.sources and flattened into
# corpus/<repo>/, preserving file names so `classify` sees the right dialect.
# Only corpus/handwritten/ is committed.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
corpus="$root/corpus"
sources="$corpus/.sources"

repos=(
  "bazelbuild/bazel"
  "bazel-contrib/rules_cc"
  "bazelbuild/rules_python"
  "bazelbuild/rules_go"
  "bazelbuild/bazel-skylib"
  "bazel-contrib/buildtools"
  "bazelbuild/rules_java"
  "bazelbuild/apple_support"
)

mkdir -p "$sources"

for repo in "${repos[@]}"; do
  name="${repo##*/}"
  if [ ! -d "$sources/$name" ]; then
    echo "cloning $repo"
    git clone --depth=1 --quiet "https://github.com/$repo.git" "$sources/$name"
  fi
done

count=0
for repo in "${repos[@]}"; do
  name="${repo##*/}"
  dest="$corpus/$name"
  rm -rf "$dest"
  mkdir -p "$dest"
  while IFS= read -r -d '' file; do
    rel="${file#"$sources/$name/"}"
    flat="${rel//\//__}"
    cp "$file" "$dest/$flat"
    count=$((count + 1))
  done < <(find "$sources/$name" \
    -path '*/.git' -prune -o \
    -type f \( -name 'BUILD' -o -name 'BUILD.bazel' -o -name '*.bzl' \
    -o -name 'MODULE.bazel' -o -name 'WORKSPACE' -o -name 'WORKSPACE.bazel' \
    -o -name '*.scl' \) -print0)
done

echo "harvested $count files into $corpus"
