#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source_root="${CATNAP_DOCKER_BINARY_SOURCE_ROOT:-${repo_root}/target}"
out_root="${CATNAP_DOCKER_BINARY_OUTPUT_ROOT:-${repo_root}/dist/docker}"
arches_csv="${CATNAP_DOCKER_BINARY_ARCHES:-amd64,arm64}"
amd64_src="${CATNAP_DOCKER_BINARY_AMD64_SOURCE:-${source_root}/x86_64-unknown-linux-gnu/release/catnap}"
arm64_src="${CATNAP_DOCKER_BINARY_ARM64_SOURCE:-${source_root}/aarch64-unknown-linux-gnu/release/catnap}"

generate_runtime_dockerfile() {
  cat > "${out_root}/Dockerfile.release" <<'EOF'
# syntax=docker/dockerfile:1

FROM debian:13-slim AS runtime
RUN set -eux;   apt-get update;   apt-get install -y --no-install-recommends ca-certificates;   (apt-get install -y --no-install-recommends libssl3 || apt-get install -y --no-install-recommends libssl3t64);   rm -rf /var/lib/apt/lists/*

WORKDIR /app
ARG TARGETARCH
COPY linux_${TARGETARCH}/catnap /app/catnap
RUN chmod +x /app/catnap

ARG APP_EFFECTIVE_VERSION=0.0.0
ENV APP_EFFECTIVE_VERSION=${APP_EFFECTIVE_VERSION}
ENV BIND_ADDR=0.0.0.0:18080

EXPOSE 18080
CMD ["/app/catnap"]
EOF
}

copy_one() {
  local arch="$1"
  local src="$2"
  local dest_dir="${out_root}/linux_${arch}"
  local dest="${dest_dir}/catnap"

  if [[ ! -f "${src}" ]]; then
    echo "missing prebuilt docker binary for ${arch}: ${src}" >&2
    exit 1
  fi

  mkdir -p "${dest_dir}"
  cp "${src}" "${dest}"
  chmod +x "${dest}"
}

rm -rf "${out_root}"
mkdir -p "${out_root}"
for arch in ${arches_csv//,/ }; do
  case "${arch}" in
    amd64)
      copy_one amd64 "${amd64_src}"
      ;;
    arm64)
      copy_one arm64 "${arm64_src}"
      ;;
    *)
      echo "unsupported docker binary arch: ${arch}" >&2
      exit 1
      ;;
  esac
done
generate_runtime_dockerfile

echo "Prepared Docker runtime binaries under ${out_root}" >&2
