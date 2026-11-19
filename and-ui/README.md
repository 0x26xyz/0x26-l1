# `&` Admin UI

Standalone operator UI for the `&` chain. It is intentionally **separate** from
the Kombat frontend (`src/frontend`) and is expected to eventually move to its
own repository and deployment.

This app talks directly to a node's JSON-RPC endpoint plus its `GET /events`
SSE feed. It does not depend on any Kombat backend services.

## Requirements

- Node.js 20+ and npm
- A reachable `&` node exposing:
  - JSON-RPC at `$RPC_URL`
  - SSE events at `$RPC_URL/events`

## Environment

| Var | Purpose | Default |
| --- | --- | --- |
| `VITE_AND_RPC_URL` | Baked-in default endpoint for first load | `https://testnet.0x26.xyz/api` |

The endpoint is also editable from the header and persisted in
`localStorage` under `and-admin-rpc-url`, so the env var is only used the
first time a browser visits the app.

## Run

```bash
cd chain/and-ui
npm install
npm run dev       # http://localhost:4174
```

### Against a local `&` node

The node exposes its JSON-RPC on `127.0.0.1:8545` by default with no CORS
headers, so in dev the UI talks to it through a Vite proxy at `/rpc`.

Terminal 1 — start the node with dev auto-production enabled:

```bash
cd chain
./scripts/kombat_demo.sh start
```

Terminal 2 — start the UI pointed at the proxy path:

```bash
cd chain/and-ui
VITE_AND_RPC_URL=/rpc npm run dev
```

Open `http://localhost:4174`. Head height should tick up as the node
auto-produces blocks; `Event cursor` will advance as the SSE stream opens.

Optional — submit some activity so the event stream has content:

```bash
cd chain
./scripts/kombat_demo.sh bootstrap           # create account + policy + intent
./scripts/kombat_demo.sh conditional-transfer
```

To point the proxy at a non-default node address, set
`VITE_AND_NODE_TARGET=http://<host>:<port>` before `npm run dev`.

Production build:

```bash
npm run build     # runs tsc then vite build -> dist/
npm run preview   # serve the built bundle
```

## Runtime behaviour

- Polls `kombat_chainInfo` and `kombat_getLiquidationPressureSummary` every
  5 seconds (see `src/config.ts`).
- Holds an SSE connection to `/events?follow=true&limit=12` and renders the
  last 12 runtime events with a connection-status indicator.
- A manual `Refresh now` button forces an immediate poll.
- Endpoint changes wipe state and reconnect.

## Scope

- No auth flows yet — this UI assumes a trusted operator network.
- No wiring into the Kombat frontend or any Kombat backend.
- No chain writes — read-only operator view.
