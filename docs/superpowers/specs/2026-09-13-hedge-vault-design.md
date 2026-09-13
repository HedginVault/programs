# Hedge Vault — Design

Protocol-gated fund-management vault on Solana, modelled on MirrorFi's code
standard but with off-chain NAV. Anchor 0.31.1.

## Goals

- Managers whitelisted by the protocol create vaults; users deposit a single
  deposit mint and receive a tokenized share (SPL mint owned by the vault).
- Managers deploy funds into Meteora DLMM positions and Jupiter swaps.
  Strategy accounts are records only; no on-chain PnL accounting.
- A centralized NAV updater posts vault total assets once per 24h epoch.
  The program derives NAV per share and settles fees as share dilution.
- Deposits and withdrawals are requests, resolvable only with a NAV posted in
  a later epoch than the request.

## Accounts

| Account | Seeds | Kind | Purpose |
| --- | --- | --- | --- |
| Config | `["config"]` | zero-copy | admin, nav_updater, treasury_authority, guardian, platform fee bps, NAV safety bps, status, next_vault_id |
| Manager | `["manager", authority]` | account | whitelist marker created/closed by admin |
| Vault | `["vault", id]` | zero-copy | metadata, share_mint, NAV state, pending totals, unclaimed fee shares |
| share mint | `["share_mint", vault]` | SPL mint | vault shares, decimals = deposit mint decimals, authority = vault |
| deposit escrow | `["deposit_escrow", vault]` | token acct | pending deposit tokens, authority = vault |
| share escrow | `["share_escrow", vault]` | token acct | pending withdrawal shares, authority = vault |
| Strategy | `["strategy", vault, protocol_account]` | account | JupiterSwap{target_mint} or MeteoraDlmm{position} |
| DepositRequest | `["deposit_request", vault, user]` | account | amount, epoch |
| WithdrawalRequest | `["withdrawal_request", vault, user]` | account | shares, epoch |

Vault working balances are ordinary ATAs owned by the vault PDA.

## NAV

- `NAV_PRECISION = 1e9`, `EPOCH_DURATION = 86_400s`, `epoch = now / EPOCH_DURATION`.
- `update_nav(total_assets)` by `config.nav_updater`, once per epoch
  (`epoch > vault.nav_epoch`).
- `supply = share_mint.supply + unclaimed fee shares` (virtual supply).
- Fees (manager: `vault.performance_fee_bps`, `vault.management_fee_bps`;
  platform: `config.platform_performance_fee_bps`,
  `config.platform_management_fee_bps`, management annualized):
  - `mgmt_fee = total_assets * bps * elapsed / (SECONDS_PER_YEAR * MAX_BPS)`
  - `perf_fee = max(0, gross_nav - high_water_mark) * supply / P * bps / MAX_BPS`
  - fee shares `= fee * supply / (total_assets - total_fee)`, accrued to
    `unclaimed_manager_fee_shares` / `unclaimed_platform_fee_shares`.
- `nav = total_assets * P / (supply + fee_shares)`; `high_water_mark = max(hwm, nav)`.
- Resolutions keep `total_assets` in sync (+amount on deposit, -amount on
  withdrawal) so NAV stays constant between updates.

## Request flow

- `request_deposit(amount)`: tokens -> deposit escrow; request keyed by epoch.
  A second request in the same epoch merges; an unresolved older request blocks.
- `request_withdrawal(shares)`: shares -> share escrow; same rules.
- `cancel_*_request`: only while `vault.nav_epoch <= request.epoch`.
- `resolve_*_request`: permissionless, requires `vault.nav_epoch > request.epoch`.
  Deposit: escrow -> vault ATA, mint `amount * P / nav` shares to user.
  Withdrawal: burn shares from escrow, transfer `shares * nav / P` from vault ATA.
  Request closed, rent to the request owner.

## Strategies (manager only)

- Jupiter: `initialize/execute/exit_strategy_jupiter_swap` as MirrorFi, minus PnL.
- Meteora DLMM: `initialize_strategy_meteora_dlmm` (initialize_position2),
  `execute_strategy_meteora_dlmm` (add_liquidity_by_strategy2 from vault X/Y
  ATAs), `exit_strategy_meteora_dlmm` (remove_liquidity_by_range2 + claim_fee2).
- `close_strategy`: closes protocol accounts by type via remaining accounts.

## NAV safety

The updater is centralized, so `update_nav` is hardened in layers:

1. `total_assets` must be at least the vault token account's idle balance.
2. NAV per share may move at most `config.max_nav_deviation_bps` per update.
   `override_nav`, signed by the config admin, bypasses only this bound.
3. Withdrawal resolutions since the last NAV update may pay out at most
   `config.max_epoch_outflow_bps` of total assets (`vault.epoch_outflow`).
4. `config.guardian` may call `pause_protocol` and nothing else; only the admin
   can unpause via `update_config`.

## Fees claim

- `claim_manager_fee` (vault authority) and `claim_platform_fee`
  (config.treasury_authority) mint unclaimed fee shares to the claimant's
  share ATA. Claimants exit through the normal withdrawal request flow.

## Security hardening (2026-09-14)

From the Solana Foundation async vault comparison:

- Deposit mint allowlist for Token-2022 extensions, checked at vault creation and on every deposit request.
- Deposits close while `nav_per_share == 0`; such deposits become cancellable; resolutions that mint 0 shares fail.
- Per-vault `min_deposit` and `min_withdrawal_shares` (full-balance withdrawals always allowed).
- Manager fee increases apply 7 days later at the first NAV update; decreases are immediate.
- Two-step admin transfer (`update_config.pending_admin` → `accept_admin`).
- `reject_deposit_request` / `reject_withdrawal_request` for the admin.
- Vault-signed CPIs run after the vault account borrow is released (resolve, cancel, fee claim and reject handlers).
- Unit tests (`cargo test -p hedge_vault`) and LiteSVM integration tests (`tests/litesvm`).
- Parked: pricing a request at the first NAV after it (settlement at `update_nav`).

## Out of scope

Locked profit, deposit/withdrawal fees, treasury PDA, Kamino, DAMM v2,
Flash Trade, deployment.
