#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  configure_testnet_rpc_failover.sh <generated-assets-dir> <ssh-host>

Description:
  Installs an Nginx config on <ssh-host> that proxies testnet.0x26.xyz/api
  across the validator RPC endpoints listed in <generated-assets-dir>/hosts.txt.
  KOMBAT_MARKET_DATA_API_UPSTREAM may point /api/market-data/candles at the
  QuestDB-backed market-data service; default is 127.0.0.1:8787.
  KOMBAT_TESTNET_RPC_PRIMARY_NAME may name the primary write endpoint.
  KOMBAT_TESTNET_RPC_EXCLUDE_NAMES may contain comma-separated validator names
  to leave out of both primary and event upstreams.
EOF
}

if [[ $# -ne 2 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
SSH_HOST="$2"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"
MARKET_DATA_API_UPSTREAM="${KOMBAT_MARKET_DATA_API_UPSTREAM:-127.0.0.1:8787}"
PRIMARY_NAME="${KOMBAT_TESTNET_RPC_PRIMARY_NAME:-}"
EXCLUDE_NAMES=",${KOMBAT_TESTNET_RPC_EXCLUDE_NAMES:-},"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "configure_testnet_rpc_failover.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require awk
require ssh
require scp

name_is_excluded() {
  [[ "${EXCLUDE_NAMES}" == *",$1,"* ]]
}

PRIMARY_UPSTREAM_LINE=""
EVENT_UPSTREAM_LINES=()
host_index=0
primary_name_found=0
while IFS=$'\t' read -r name ssh_host public_rpc_url; do
  if [[ "${name}" == "name" ]]; then
    continue
  fi
  if name_is_excluded "${name}"; then
    host_index=$((host_index + 1))
    continue
  fi
  host_port="${public_rpc_url#http://}"
  host_port="${host_port#https://}"
  upstream_host_port="${host_port}"
  if [[ ${host_index} -eq 0 ]]; then
    upstream_host_port="127.0.0.1:8545"
  fi
  EVENT_UPSTREAM_LINES+=("    server ${upstream_host_port} max_fails=1 fail_timeout=2s;")
  if [[ -n "${PRIMARY_NAME}" && "${name}" == "${PRIMARY_NAME}" ]]; then
    PRIMARY_UPSTREAM_LINE="    server ${upstream_host_port} max_fails=1 fail_timeout=2s;"
    primary_name_found=1
  elif [[ -z "${PRIMARY_NAME}" && -z "${PRIMARY_UPSTREAM_LINE}" ]]; then
    PRIMARY_UPSTREAM_LINE="    server ${upstream_host_port} max_fails=1 fail_timeout=2s;"
  fi
  host_index=$((host_index + 1))
done < "${HOSTS_FILE}"

if [[ -z "${PRIMARY_UPSTREAM_LINE}" ]]; then
  echo "configure_testnet_rpc_failover.sh: no upstream validators found" >&2
  exit 1
fi
if [[ -n "${PRIMARY_NAME}" && ${primary_name_found} -ne 1 ]]; then
  echo "configure_testnet_rpc_failover.sh: primary validator ${PRIMARY_NAME} not found in non-excluded upstreams" >&2
  exit 1
fi

TMP_FILE="$(mktemp -t testnet-nginx-failover.XXXXXX)"
trap 'rm -f "${TMP_FILE}"' EXIT

{
  cat <<'EOF'
map $http_origin $and_chain_cors_origin {
    default "https://0x26.xyz";
    "https://0x26.xyz" $http_origin;
    "https://testnet.0x26.xyz" $http_origin;
    "http://localhost:3000" $http_origin;
    "http://127.0.0.1:3000" $http_origin;
    "http://localhost:5173" $http_origin;
    "http://127.0.0.1:5173" $http_origin;
}

upstream and_testnet_rpc_events {
    least_conn;
EOF
  printf '%s\n' "${EVENT_UPSTREAM_LINES[@]}"
  cat <<'EOF'
}

upstream and_testnet_rpc_primary {
EOF
  printf '%s\n' "${PRIMARY_UPSTREAM_LINE}"
  cat <<'EOF'
}

upstream and_market_data_api {
EOF
  printf '    server %s max_fails=1 fail_timeout=2s;\n' "${MARKET_DATA_API_UPSTREAM}"
  cat <<'EOF'
}

server {
    listen 80;
    listen [::]:80;
    server_name testnet.0x26.xyz;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name testnet.0x26.xyz;

    ssl_certificate /etc/nginx/ssl/testnet.0x26.xyz.crt;
    ssl_certificate_key /etc/nginx/ssl/testnet.0x26.xyz.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_session_timeout 1d;
    ssl_session_cache shared:SSL:10m;

    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

    location = / {
        default_type text/plain;
        return 200 '& chain testnet';
    }

    location = /api/events {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream error timeout http_502 http_503 http_504 invalid_header;
        proxy_next_upstream_tries 4;
        proxy_pass http://and_testnet_rpc_events/events;
    }

    location /api/events/ {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream error timeout http_502 http_503 http_504 invalid_header;
        proxy_next_upstream_tries 4;
        proxy_pass http://and_testnet_rpc_events/events/;
    }

    location = /api {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_hide_header Access-Control-Allow-Origin;
        proxy_hide_header Access-Control-Allow-Headers;
        proxy_hide_header Access-Control-Allow-Methods;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream off;
        proxy_pass http://and_testnet_rpc_primary/;
    }

    location = /api/market-data/candles {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_hide_header Access-Control-Allow-Origin;
        proxy_hide_header Access-Control-Allow-Headers;
        proxy_hide_header Access-Control-Allow-Methods;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream off;
        proxy_pass http://and_market_data_api/market-data/candles;
    }

    location /api/ {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream off;
        proxy_pass http://and_testnet_rpc_primary/;
    }

    location = /rpc/events {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream error timeout http_502 http_503 http_504 invalid_header;
        proxy_next_upstream_tries 4;
        proxy_pass http://and_testnet_rpc_events/events;
    }

    location /rpc/events/ {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream error timeout http_502 http_503 http_504 invalid_header;
        proxy_next_upstream_tries 4;
        proxy_pass http://and_testnet_rpc_events/events/;
    }

    location = /rpc {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream off;
        proxy_pass http://and_testnet_rpc_primary/;
    }

    location = /rpc/market-data/candles {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream off;
        proxy_pass http://and_market_data_api/market-data/candles;
    }

    location /rpc/ {
        if ($request_method = OPTIONS) { return 204; }
        add_header Access-Control-Allow-Origin "$and_chain_cors_origin" always;
        add_header Access-Control-Allow-Headers "content-type, authorization, last-event-id" always;
        add_header Access-Control-Allow-Methods "GET, POST, OPTIONS" always;
        add_header Vary "Origin" always;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_buffering off;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
        proxy_next_upstream off;
        proxy_pass http://and_testnet_rpc_primary/;
    }
}
EOF
} > "${TMP_FILE}"

scp "${TMP_FILE}" "${SSH_HOST}:/tmp/testnet.0x26.xyz.nginx"
ssh "${SSH_HOST}" "\
  install -o root -g root -m 0644 /tmp/testnet.0x26.xyz.nginx /etc/nginx/sites-enabled/testnet.0x26.xyz && \
  nginx -t && \
  systemctl reload nginx"

echo "configured nginx failover on ${SSH_HOST}"
