#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
script_path="${repo_root}/.github/scripts/prepare-docker-binaries.sh"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmpdir}"
}
trap cleanup EXIT

src_root="${tmpdir}/target"
out_root="${tmpdir}/dist/docker"
mkdir -p   "${src_root}/x86_64-unknown-linux-gnu/release"   "${src_root}/aarch64-unknown-linux-gnu/release"   "${src_root}/release"
printf 'amd64-cross
' > "${src_root}/x86_64-unknown-linux-gnu/release/catnap"
printf 'arm64-cross
' > "${src_root}/aarch64-unknown-linux-gnu/release/catnap"
printf 'amd64-host
' > "${src_root}/release/catnap"

CATNAP_DOCKER_BINARY_SOURCE_ROOT="${src_root}" CATNAP_DOCKER_BINARY_OUTPUT_ROOT="${out_root}"   bash "${script_path}" >/dev/null

cmp -s   "${src_root}/x86_64-unknown-linux-gnu/release/catnap"   "${out_root}/linux_amd64/catnap"
cmp -s   "${src_root}/aarch64-unknown-linux-gnu/release/catnap"   "${out_root}/linux_arm64/catnap"
[[ -x "${out_root}/linux_amd64/catnap" ]]
[[ -x "${out_root}/linux_arm64/catnap" ]]
[[ -f "${out_root}/Dockerfile.release" ]]
grep -q 'ARG TARGETARCH' "${out_root}/Dockerfile.release"
grep -q 'COPY linux_\${TARGETARCH}/catnap /app/catnap' "${out_root}/Dockerfile.release"

single_out_root="${tmpdir}/dist/docker-single"
CATNAP_DOCKER_BINARY_SOURCE_ROOT="${src_root}" CATNAP_DOCKER_BINARY_OUTPUT_ROOT="${single_out_root}" CATNAP_DOCKER_BINARY_ARCHES="amd64" CATNAP_DOCKER_BINARY_AMD64_SOURCE="${src_root}/release/catnap"   bash "${script_path}" >/dev/null
cmp -s "${src_root}/release/catnap" "${single_out_root}/linux_amd64/catnap"
[[ ! -e "${single_out_root}/linux_arm64/catnap" ]]

rm -f "${src_root}/aarch64-unknown-linux-gnu/release/catnap"
if CATNAP_DOCKER_BINARY_SOURCE_ROOT="${src_root}"    CATNAP_DOCKER_BINARY_OUTPUT_ROOT="${out_root}"    bash "${script_path}" >/dev/null 2>&1; then
  echo "expected missing arm64 binary to fail" >&2
  exit 1
fi

if CATNAP_DOCKER_BINARY_SOURCE_ROOT="${src_root}"    CATNAP_DOCKER_BINARY_OUTPUT_ROOT="${out_root}"    CATNAP_DOCKER_BINARY_ARCHES="s390x"    bash "${script_path}" >/dev/null 2>&1; then
  echo "expected unsupported arch to fail" >&2
  exit 1
fi

echo "prepare-docker-binaries tests passed"
