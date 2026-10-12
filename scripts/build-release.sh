#!/usr/bin/env bash
set -euo pipefail

# Veevi Sentinel - Production Release Build Creator
# Builds optimized cross-platform binaries, packages archives, and generates SHA256SUMS.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"

VERSION=$(grep -m1 '^version = ' "${ROOT_DIR}/Cargo.toml" | cut -d '"' -f2)
VERSION_CLEAN="${VERSION#v}"

echo "================================================================="
echo " Building Veevi Sentinel Release v${VERSION_CLEAN}"
echo "================================================================="

rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

BUILD_TARGETS=(
    "x86_64-unknown-linux-musl:linux:x86_64"
    "aarch64-unknown-linux-musl:linux:aarch64"
    "x86_64-apple-darwin:darwin:x86_64"
    "aarch64-apple-darwin:darwin:aarch64"
)

# Current Host Target Detection
HOST_OS=$(uname -s | tr '[:upper:]' '[:lower:]')
HOST_ARCH=$(uname -m)
case "$HOST_ARCH" in
    x86_64|amd64) HOST_ARCH="x86_64" ;;
    arm64|aarch64) HOST_ARCH="aarch64" ;;
esac

echo "Host Platform: ${HOST_OS}-${HOST_ARCH}"

# Function to build and package target
package_binary() {
    local target_triple="$1"
    local os_name="$2"
    local arch_name="$3"
    local archive_name="veevi-sentinel-${VERSION_CLEAN}-${os_name}-${arch_name}.tar.gz"
    local stage_dir="${DIST_DIR}/stage_${os_name}_${arch_name}"

    echo ""
    echo "==> Packaging ${os_name}/${arch_name} (${target_triple})..."

    mkdir -p "${stage_dir}"

    local binary_path=""
    if [ "${target_triple}" = "host" ]; then
        cargo build --release --manifest-path "${ROOT_DIR}/Cargo.toml"
        binary_path="${ROOT_DIR}/target/release/veevi-sentinel"
    elif cargo build --release --target "${target_triple}" --manifest-path "${ROOT_DIR}/Cargo.toml" 2>/dev/null; then
        binary_path="${ROOT_DIR}/target/${target_triple}/release/veevi-sentinel"
    elif command -v cross >/dev/null 2>&1; then
        echo "Using 'cross' for cross-compilation..."
        cross build --release --target "${target_triple}" --manifest-path "${ROOT_DIR}/Cargo.toml"
        binary_path="${ROOT_DIR}/target/${target_triple}/release/veevi-sentinel"
    else
        echo "⚠️ Target ${target_triple} cannot be compiled natively on this host and 'cross' is not installed."
        echo "   Skipping ${archive_name} for this run."
        rm -rf "${stage_dir}"
        return 0
    fi

    if [ -f "${binary_path}" ]; then
        cp "${binary_path}" "${stage_dir}/veevi-sentinel"
        chmod +x "${stage_dir}/veevi-sentinel"
        cp "${ROOT_DIR}/LICENSE" "${stage_dir}/" 2>/dev/null || true
        cp "${ROOT_DIR}/README.md" "${stage_dir}/" 2>/dev/null || true

        # Build tar.gz matching install.sh spec
        (cd "${stage_dir}" && tar -czf "${DIST_DIR}/${archive_name}" veevi-sentinel)
        rm -rf "${stage_dir}"
        echo "✅ Built ${archive_name}"
    fi
}

# Check if target specified as argument
if [ $# -gt 0 ]; then
    TARGET_ARG="$1"
    package_binary "${TARGET_ARG}" "${HOST_OS}" "${HOST_ARCH}"
else
    # Build current host release binary
    package_binary "host" "${HOST_OS}" "${HOST_ARCH}"
fi

# Generate SHA256SUMS manifest
echo ""
echo "==> Generating SHA256SUMS manifest..."
cd "${DIST_DIR}"
if [ -n "$(find . -maxdepth 1 -name '*.tar.gz' -print -quit)" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum *.tar.gz > SHA256SUMS
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 *.tar.gz > SHA256SUMS
    fi
    echo "✅ SHA256SUMS generated successfully:"
    cat SHA256SUMS
else
    echo "No release tarballs were generated."
fi

echo ""
echo "🎉 Release build creation complete! Artifacts available in: ${DIST_DIR}"
