#!/bin/bash
# gen_llms_txt.sh
# Generates llms.txt files directly in the source tree using cargo-llms-txt
# Usage: bazel run //yatws:gen_llms_txt

set -e

# BUILD_WORKING_DIRECTORY is set by Bazel when running via `bazel run`
WORKSPACE_DIR="${BUILD_WORKING_DIRECTORY:-$(pwd)}"
YATWS_DIR="${WORKSPACE_DIR}/yatws"

echo "Generating llms.txt files for yatws crate..."
echo "Source directory: ${YATWS_DIR}"

# Find the cargo-llms-txt binary
# In bazel run, the binary is available via the runfiles or in the path
CARGO_LLMS_TXT="${WORKSPACE_DIR}/bazel-bin/external/+http_archive+cargo_llms_txt/cargo-llms-txt"

# If not found, try to build it
if [ ! -f "${CARGO_LLMS_TXT}" ]; then
    echo "cargo-llms-txt not found, building..."
    cd "${WORKSPACE_DIR}"
    bazel build @cargo_llms_txt//:cargo-llms-txt
fi

# Verify binary exists
if [ ! -f "${CARGO_LLMS_TXT}" ]; then
    echo "Error: cargo-llms-txt binary not found at ${CARGO_LLMS_TXT}"
    exit 1
fi

# Run the tool on the yatws directory
echo "Running cargo-llms-txt..."
cd "${YATWS_DIR}"
"${CARGO_LLMS_TXT}" -p "${YATWS_DIR}"

# Make files writable
chmod 644 "${YATWS_DIR}/llms.txt" 2>/dev/null || true
chmod 644 "${YATWS_DIR}/llms-full.txt" 2>/dev/null || true

echo "Done! Files generated:"
echo "  - ${YATWS_DIR}/llms.txt"
echo "  - ${YATWS_DIR}/llms-full.txt"
