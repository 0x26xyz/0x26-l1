#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  capture_remote_loadgen_profile.sh <ssh-host> <local-profile-dir> [profile-secs] [interval-secs]

Description:
  Captures process, socket, CPU, and network samples from a remote loadgen host
  during a live benchmark. The resulting archive is fetched into
  <local-profile-dir>/<safe-host>.tgz.
EOF
}

if [[ $# -lt 2 || $# -gt 4 ]]; then
  usage >&2
  exit 1
fi

SSH_HOST="$1"
LOCAL_PROFILE_DIR="$2"
PROFILE_SECS="${3:-105}"
INTERVAL_SECS="${4:-2}"

if ! [[ "${PROFILE_SECS}" =~ ^[0-9]+$ ]] || [[ "${PROFILE_SECS}" -eq 0 ]]; then
  echo "capture_remote_loadgen_profile.sh: profile-secs must be positive" >&2
  exit 1
fi
if ! [[ "${INTERVAL_SECS}" =~ ^[0-9]+$ ]] || [[ "${INTERVAL_SECS}" -eq 0 ]]; then
  echo "capture_remote_loadgen_profile.sh: interval-secs must be positive" >&2
  exit 1
fi

mkdir -p "${LOCAL_PROFILE_DIR}"

SAFE_HOST="$(printf '%s' "${SSH_HOST}" | tr '@/:' '___')"
REMOTE_DIR="/tmp/kombat-loadgen-profile-${SAFE_HOST}"
REMOTE_TAR="${REMOTE_DIR}.tgz"
LOCAL_TAR="${LOCAL_PROFILE_DIR}/${SAFE_HOST}.tgz"

ssh "${SSH_HOST}" "rm -rf '${REMOTE_DIR}' '${REMOTE_TAR}' && mkdir -p '${REMOTE_DIR}/samples'"

ssh "${SSH_HOST}" \
  "PROFILE_SECS='${PROFILE_SECS}' INTERVAL_SECS='${INTERVAL_SECS}' REMOTE_DIR='${REMOTE_DIR}' REMOTE_TAR='${REMOTE_TAR}' bash -s" <<'REMOTE'
set -euo pipefail

{
  echo "captured_at_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host=$(hostname)"
  echo "uname=$(uname -a)"
  echo "profile_secs=${PROFILE_SECS}"
  echo "interval_secs=${INTERVAL_SECS}"
  echo
  echo "## lscpu"
  lscpu || true
  echo
  echo "## free -m"
  free -m || true
  echo
  echo "## selected sysctl"
  sysctl \
    fs.file-max \
    net.core.rmem_max \
    net.core.wmem_max \
    net.core.somaxconn \
    net.core.netdev_max_backlog \
    net.ipv4.ip_local_port_range \
    net.ipv4.tcp_fin_timeout \
    net.ipv4.tcp_tw_reuse 2>/dev/null || true
} > "${REMOTE_DIR}/static.txt"

end=$((SECONDS + PROFILE_SECS))
idx=0
while [ "${SECONDS}" -lt "${end}" ]; do
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  sample="${REMOTE_DIR}/samples/$(printf '%04d' "${idx}")-${ts}.txt"
  {
    echo "sample_index=${idx}"
    echo "sample_utc=${ts}"
    echo
    echo "## uptime"
    uptime || true
    echo
    echo "## proc loadavg"
    cat /proc/loadavg || true
    echo
    echo "## vmstat"
    vmstat 1 2 | tail -n 1 || true
    echo
    echo "## mpstat summary"
    mpstat 1 1 2>/dev/null || true
    echo
    echo "## loadgen processes"
    ps -eo pid,ppid,cls,pri,ni,psr,pcpu,pmem,rss,vsz,etime,comm,args --sort=-pcpu \
      | awk 'NR == 1 || /kombat_loadgen|loadgen/' || true
    echo
    echo "## top loadgen threads"
    ps -eLo pid,tid,psr,pcpu,pmem,comm,args --sort=-pcpu \
      | awk 'NR == 1 || /kombat_loadgen|loadgen/' \
      | head -n 200 || true
    echo
    echo "## top processes"
    ps -eo pid,ppid,cls,pri,ni,psr,pcpu,pmem,rss,vsz,etime,comm,args --sort=-pcpu \
      | head -n 160 || true
    echo
    echo "## sockets summary"
    ss -s || true
    echo
    echo "## established chain sockets"
    ss -antp | egrep ':(8545|8645|8646|8647|8648|8649|8650)' | head -n 300 || true
    echo
    echo "## softirqs"
    cat /proc/softirqs || true
    echo
    echo "## net dev"
    cat /proc/net/dev || true
    echo
    echo "## ip link stats"
    ip -s link || true
    echo
    echo "## snmp"
    cat /proc/net/snmp || true
  } > "${sample}"
  idx=$((idx + 1))
  sleep "${INTERVAL_SECS}"
done

tar -C "${REMOTE_DIR}" -czf "${REMOTE_TAR}" .
REMOTE

scp "${SSH_HOST}:${REMOTE_TAR}" "${LOCAL_TAR}"
printf '%s\n' "${LOCAL_TAR}"
