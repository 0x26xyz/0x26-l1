#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_DIR="$(cd "${CHAIN_DIR}/.." && pwd)"

IMAGE="${KOMBAT_LINUX_BUILDER_IMAGE:-kombat-chain-linux-builder:1.93}"
TARGET_DIR="${KOMBAT_LINUX_TARGET_DIR:-target-linux-x86_64}"
BUILD_DIR="${TARGET_DIR}/build"
RELEASE_DIR="${TARGET_DIR}/release"

docker build \
  --platform linux/amd64 \
  -t "${IMAGE}" \
  -f "${CHAIN_DIR}/deploy/docker/Dockerfile.linux-builder" \
  "${REPO_DIR}"

docker run --rm \
  --platform linux/amd64 \
  -v "${CHAIN_DIR}:/work" \
  -w /work \
  "${IMAGE}" \
  bash -lc "set -euo pipefail; \
    export PATH=/usr/local/cargo/bin:\$PATH; \
    CARGO_TARGET_DIR=${BUILD_DIR} cargo build --release \
      -p kombat-chain-rpc --bin kombat_chain --bin kombat_sign \
      -p kombat-chain-market-data --bin kombat_market_data \
      -p kombat-chain-feeder --bin kombat_feeder \
      -p kombat-chain-loadgen --bin kombat_loadgen --bin kombat_mm_bot --bin kombat_funding_arb_bot; \
    mkdir -p ${RELEASE_DIR}; \
    cp ${BUILD_DIR}/release/kombat_chain ${RELEASE_DIR}/kombat_chain; \
    cp ${BUILD_DIR}/release/kombat_sign ${RELEASE_DIR}/kombat_sign; \
    cp ${BUILD_DIR}/release/kombat_market_data ${RELEASE_DIR}/kombat_market_data; \
    cp ${BUILD_DIR}/release/kombat_feeder ${RELEASE_DIR}/kombat_feeder; \
    cp ${BUILD_DIR}/release/kombat_loadgen ${RELEASE_DIR}/kombat_loadgen; \
    cp ${BUILD_DIR}/release/kombat_mm_bot ${RELEASE_DIR}/kombat_mm_bot; \
    cp ${BUILD_DIR}/release/kombat_funding_arb_bot ${RELEASE_DIR}/kombat_funding_arb_bot"

sha256sum \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_chain" \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_sign" \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_market_data" \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_feeder" \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_loadgen" \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_mm_bot" \
  "${CHAIN_DIR}/${RELEASE_DIR}/kombat_funding_arb_bot"
