#!/bin/bash -eu
#
# Build all cargo-fuzz targets and copy them to $OUT.
# cargo-fuzz applies sanitizer/coverage flags from the ClusterFuzzLite
# environment; do not pass extra -Z sanitizer flags here.

cd "$SRC/happy-cracking"

cargo fuzz build -O --debug-assertions

FUZZ_TARGET_OUTPUT_DIR="fuzz/target/x86_64-unknown-linux-gnu/release"
if [[ ! -d "$FUZZ_TARGET_OUTPUT_DIR" ]]; then
  FUZZ_TARGET_OUTPUT_DIR="target/x86_64-unknown-linux-gnu/release"
fi

for f in fuzz/fuzz_targets/*.rs; do
  FUZZ_TARGET_NAME="$(basename "${f%.*}")"
  cp "${FUZZ_TARGET_OUTPUT_DIR}/${FUZZ_TARGET_NAME}" "$OUT/"
  corpus_dir="fuzz/corpus/${FUZZ_TARGET_NAME}"
  if [[ -d "$corpus_dir" ]] && compgen -G "${corpus_dir}/*" > /dev/null; then
    zip -j "$OUT/${FUZZ_TARGET_NAME}_seed_corpus.zip" "${corpus_dir}"/*
  fi
done
