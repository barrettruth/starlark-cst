#!/usr/bin/env bash
# Populate corpus/ with real-world Bazel files.
#
# Sources are shallow-cloned into corpus/.sources and copied into corpus/<repo>/
# with their directory structure intact. The structure matters: `classify`
# dispatches on the file name, so a flattened `pkg__BUILD` stops being a BUILD
# file and silently leaves the corpus.
#
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
    mkdir -p "$dest/$(dirname "$rel")"
    cp "$file" "$dest/$rel"
    count=$((count + 1))
  done < <(find "$sources/$name" \
    -path '*/.git' -prune -o \
    -type f \( -name 'BUILD' -o -name 'BUILD.bazel' -o -name '*.bzl' \
    -o -name 'MODULE.bazel' -o -name 'WORKSPACE' -o -name 'WORKSPACE.bazel' \
    -o -name '*.scl' \) -print0)
done

echo "harvested $count files into $corpus"
