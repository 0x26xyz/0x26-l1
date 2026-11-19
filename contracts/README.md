# Arena Settlement Contracts

Foundry project for GTA / Arena on-chain settlement.

## Setup

```bash
cd chain/contracts
forge install foundry-rs/forge-std --no-git
forge build
forge test
```

## Deploy

```bash
cd chain/contracts
export PRIVATE_KEY=...
export RPC_URL=https://testnet.0x26.xyz/api
export USDC_ADDRESS=0x0000000000000000000000000000000000001014
forge script script/DeployArenaSettlement.s.sol:DeployArenaSettlement \
  --rpc-url "$RPC_URL" \
  --broadcast
```

Optional:

- `OPERATOR_ADDRESS=0x...` to grant a backend/operator EOA immediately
- `TREASURY_SEED_USDC=1000` to seed treasury from deployer wallet

## Local dev bootstrap

The repo includes a one-shot local bootstrap that:
- seeds policy-side and EVM-side USDC through operator RPC
- deploys `ArenaSettlement`
- funds the settlement treasury
- writes an env file for `chat-service` and `frontend`

```bash
./scripts/arena_local_bootstrap.sh
source /tmp/pvp-arena-local.env
```

## Shared testnet prepare

Use the testnet helper when the public RPC is up:

```bash
export OPERATOR_PRIVATE_KEY=0x...
export PLAYER_ADDRESS=0x...

# If you have operator RPC access on testnet and want the script to seed 0x1014 as well:
export ENABLE_OPERATOR_RPC_BOOTSTRAP=true
export OPERATOR_TOKEN=...
export OPERATOR_POLICY_USDC_MICRO=5000000000
export OPERATOR_EVM_USDC_MICRO=2000000000
export PLAYER_EVM_USDC_MICRO=200000000

# Optional if the deployer already has EVM USDC:
export TREASURY_SEED_USDC_MICRO=1000000000

./scripts/arena_testnet_prepare.sh
source /tmp/pvp-arena-testnet.env
```

Defaults:
- `CHAIN_RPC_URL=https://testnet.0x26.xyz/api`
- `CHAIN_ID=38`
- `USDC_ADDRESS=0x0000000000000000000000000000000000001014`

If `ENABLE_OPERATOR_RPC_BOOTSTRAP=false` (the default), the script only deploys the contract and emits the env file. In that mode, the deployer/player wallets must already hold EVM USDC at `0x1014`.

Readiness check before attempting GTA on shared testnet:

```bash
curl -sS -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' \
  https://testnet.0x26.xyz/api
```

Expected:
- a real `eth_chainId` result for chain `38`
- not `{"code":-32601,"message":"method not found"}`

If `eth_*` methods are missing, the validator/ingress rollout is still on the older non-EVM RPC build and GTA cannot work there yet, regardless of app configuration.
