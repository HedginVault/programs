# Hedge Vault

Protocol-gated fund management vault on Solana. Whitelisted managers deploy user
deposits into Meteora DLMM positions and Jupiter swaps. NAV is computed
off-chain and posted once per 4h epoch by a centralized updater; deposits and
withdrawals are requests resolved at the next posted NAV.

Docs: [design spec](docs/superpowers/specs/2026-09-13-hedge-vault-design.md) ·
[account data model](docs/accounts.md) ·
[architecture evolution](docs/architecture-evolution.md) ·

## Flows

**Admin**
`config_initialize` → `config_update` / `config_add_manager` / `config_remove_manager` / `config_claim_platform_fee` / `nav_override`

**NAV updater**
`nav_update(total_assets)` once per epoch per vault. Settles management and
performance fees as share dilution, stores `nav_per_share`. Rejected if total
assets are below the vault's idle balance or NAV moves more than
`max_nav_deviation_bps`; the admin can `nav_override` past the bound.
Withdrawals are capped per epoch by `max_epoch_outflow_bps`.

**Guardian**
`config_pause` only. Unpausing is admin-only via `config_update`.

**Manager**
`vault_initialize` (requires Manager PDA) → `vault_update` / `vault_close` / `vault_claim_manager_fee`
Strategies: `jupiter_initialize_strategy` / `jupiter_swap`,
`meteora_dlmm_initialize_position` / `meteora_dlmm_add_liquidity` / `meteora_dlmm_remove_liquidity` /
`meteora_dlmm_claim_fee` (10% of claimed fees to the treasury), `vault_close_strategy`.

DLMM positions start at 1–70 bins. The manager can call `meteora_dlmm_extend_position`
repeatedly to add 1–91 bins to the upper end per transaction, up to 1,400 bins total.
The authority pays the extra account rent. Confirm each extension before using the new
range for liquidity. Hedge Vault rejects `meteora_dlmm_add_liquidity` ranges above
91 bins; clients must split wide add/remove/claim work into transactions that fit Solana's
transaction size and compute limits. For partial unwinds and fee
claims, use `meteora_dlmm_remove_liquidity_range` and `meteora_dlmm_claim_fee_range`
with inclusive bin bounds inside the position. The original instructions still
cover the full position range.

**User**
`deposit_request_create` → (NAV posted in a later epoch) → `deposit_request_resolve`
`withdrawal_request_create` → (NAV posted in a later epoch) → `withdrawal_request_resolve`
`*_request_cancel` is allowed only until a NAV covering the request is posted.
Resolution is permissionless.

## Build

```sh
anchor build
```

## QA handlers

One script per instruction under `tests/handler/`. Each builds a single
transaction, simulates it, prints the logs, and only lands it when `SEND=true`.

1. Set the cluster and wallet in `Anchor.toml` `[provider]`.
2. Fill in the addresses under test in `tests/handler/params.ts`.
3. Run a handler by instruction name:

```sh
anchor run initialize-config
SEND=true anchor run initialize-config
```

Jupiter handlers use the public `lite-api.jup.ag` endpoint unless
`JUPITER_API_BASE_URL` / `JUPITER_API_KEY` are set (see `.env.example`).

Protocol IDLs consumed via `declare_program!` live in `idls/`.
