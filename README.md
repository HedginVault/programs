# Hedge Vault

Protocol-gated fund management vault on Solana. Whitelisted managers deploy user
deposits into Meteora DLMM positions and Jupiter swaps. NAV is computed
off-chain and posted once per 24h epoch by a centralized updater; deposits and
withdrawals are requests resolved at the next posted NAV.

Design notes: [docs/superpowers/specs/2026-09-13-hedge-vault-design.md](docs/superpowers/specs/2026-09-13-hedge-vault-design.md)

## Flows

**Admin**
`initialize_config` → `update_config` / `add_manager` / `remove_manager` / `claim_platform_fee` / `override_nav`

**NAV updater**
`update_nav(total_assets)` once per epoch per vault. Settles management and
performance fees as share dilution, stores `nav_per_share`. Rejected if total
assets are below the vault's idle balance or NAV moves more than
`max_nav_deviation_bps`; the admin can `override_nav` past the bound.
Withdrawals are capped per epoch by `max_epoch_outflow_bps`.

**Guardian**
`pause_protocol` only. Unpausing is admin-only via `update_config`.

**Manager**
`initialize_vault` (requires Manager PDA) → `update_vault` / `close_vault` / `claim_manager_fee`
Strategies: `initialize_/execute_/exit_strategy_jupiter_swap`,
`initialize_/execute_/exit_strategy_meteora_dlmm`, `close_strategy`.

**User**
`request_deposit` → (NAV posted in a later epoch) → `resolve_deposit_request`
`request_withdrawal` → (NAV posted in a later epoch) → `resolve_withdrawal_request`
`cancel_*_request` is allowed only until a NAV covering the request is posted.
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
