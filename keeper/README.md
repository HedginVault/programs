# Hedge Vault — NAV Keeper

Backend service that values every `hedge_vault` vault once per 24 h epoch and posts `nav_update`
as the protocol's `nav_updater`. No UI. Every run is recorded in Postgres.

## What it does

Every minute the keeper:

1. Reads `Config`, checks its key is still the `nav_updater`, checks its SOL balance, and skips the
   tick if the protocol is paused.
2. Reads every vault. A vault is due when its `nav_epoch` is behind the current epoch
   (`unix_ts / 86400`) and the epoch started more than five minutes ago.
3. For each due vault, values it from one account snapshot and one price request (below),
   simulates `nav_update(total_assets)`, and sends it only on a clean simulation. The outcome is
   upserted into `nav_runs` and alerted on status change.

### What counts as an asset

Only what the vault's `Strategy` records point to, plus the idle balance:

| Holding | Source | Rule |
| --- | --- | --- |
| Idle | vault ATA for the deposit mint | face value |
| Jupiter strategy | vault ATA for `target_mint` | price(target) / price(deposit) via Jupiter |
| DLMM strategy | position in the strategy | X and Y amounts in full, pending fees at 90 % (treasury takes 10 % on claim) |

Tokens sent to the vault out of band are never counted, so they cannot move NAV. A missing price
for any counted mint aborts the vault's run; the keeper never posts a partial valuation.

### One snapshot per valuation

Amounts read at different slots can count a manager's idle → DLMM move twice or not at all, so a
valuation has three steps:

1. **Lookup** finds addresses only: the strategies, each position's pair and bin range, the pool
   mints, and the token programs for the ATAs. No amount from this step is used.
2. **Snapshot** reads, in a single `getMultipleAccountsInfo`, the clock, the vault, every strategy,
   the idle and target ATAs, every position, pair and covered bin array, and every mint. Above 100
   accounts the later chunks are pinned to the first chunk's slot with `minContextSlot`. DLMM
   amounts and pending fees are computed from these accounts with the SDK's position math, and
   the slot is stored in `nav_runs.snapshot_slot`.
3. **Prices** come from one uncached Jupiter `/price/v3` request for every mint involved (one per 50
   mints).

The run fails instead of valuing a snapshot that no longer matches the lookup (see Runbook).

**Manager rule.** A DLMM pool token that is not the deposit mint is only counted while it sits in
the position. Open a Jupiter strategy for that mint before removing liquidity (needed anyway to
swap it back), otherwise the removed amount is uncounted until one exists.

### Outcomes

| status | meaning | retried |
| --- | --- | --- |
| `posted` | landed, `nav_epoch` advanced, signature stored | no |
| `skipped` | someone else posted this epoch | no |
| `needs_override` | simulation hit `NavDeviationExceeded`; admin must `nav_override` | every tick |
| `failed` | valuation or send failed (`last_error` says why) | every tick |
| `dry_run` | `DRY_RUN=true`, simulated only | every tick |

The keeper never calls `nav_override` and never resolves requests.

### Alerts

Structured JSON logs always; `ALERT_WEBHOOK_URL` gets a JSON POST for `warn` and `error`
(successful posts are logged only). Reasons: `needs_override`, `run_failed`, `overdue`
(≥ 2 epochs behind), `protocol_paused`, `low_sol`, `updater_mismatch`, `posted`. An alert fires
when a vault's status or error changes, not on every retry.

## Run

```sh
cd keeper
yarn install
yarn sync-idl            # after every `anchor build`
cp .env.example .env     # fill in the values below
yarn dev                 # tsx, or: yarn build && yarn start
```

Docker:

```sh
docker build -t hedge-keeper keeper
docker run --env-file keeper/.env hedge-keeper
```

Migrations in `migrations/` apply automatically at startup.

## Environment

| Variable | Purpose |
| --- | --- |
| `RPC_URL` | reads, simulation, send |
| `KEEPER_KEYPAIR` | JSON array of 64 u8, the `nav_updater` secret key: `$(cat ~/.config/solana/keeper.json)` |
| `DATABASE_URL` | Postgres connection string |
| `PROGRAM_ID` | hedge_vault program id, prefilled in `.env.example` |
| `JUPITER_API_KEY` | key for `api.jup.ag`, used for token prices |
| `DRY_RUN` | `true` simulates and records without sending. Start there, flip to `false` once the breakdowns look right |
| `ALERT_WEBHOOK_URL` | optional JSON POST target for warn/error alerts |

Tuning that is not expected to change lives as constants at the top of `src/config.ts`: 60 s
tick, 5 min wait after each epoch boundary, 0.05 SOL low-balance threshold, no posting while the
protocol is paused, no webhook for successful posts.

## Layout

```
src/
  index.ts        startup checks + tick loop
  config.ts       env → KeeperConfig
  chain.ts        Connection, Program, PDAs, batched fetches
  scheduler.ts    pure due/overdue decision
  runner.ts       value → post → record → alert for one vault
  post.ts         build, simulate, classify, sign, send, confirm
  valuation/      lookup → snapshot → holdings, pure computeValuation, Pricer (Jupiter), DLMM position math
  db.ts           pg pool, migrations, nav_runs upsert
  alerts.ts       dedupe + log + webhook
migrations/       SQL, applied at startup
idl/              synced from ../target
test/             vitest
```

Pricing is behind the `Pricer` interface in `src/valuation/pricer.ts`; a future engine replaces
that one implementation.

## Runbook

- **`needs_override`**: prices moved more than `max_nav_deviation_bps` in one epoch. Check the
  `breakdown` in `nav_runs`, then the admin runs `nav_override` (`anchor run nav-override` with
  `TOTAL_ASSETS` set to the keeper's `total_assets`). The keeper records `skipped` afterwards.
- **`failed` with `missing_price:<mint>`**: Jupiter has no price for a counted mint. Retried every
  tick; if persistent, the manager should swap out of that mint or an admin override is needed.
- **`failed` with `snapshot_drift:<what>`**: a strategy was added or closed, or a position was
  resized, between the lookup and the snapshot. It clears on the next tick; if it persists, a
  manager is changing strategies continuously around the posting time.
- **`failed` with `account_missing:<pubkey>` or `bin_array_missing:<pubkey>`**: the snapshot lacked
  a mint, pair, clock, or a bin array that holds position liquidity. Usually a lagging RPC node;
  if persistent, check the account on chain.
- **`failed` with `position_missing:<pubkey>`**: an open DLMM strategy points at a closed position;
  the manager should `vault_close_strategy` it.
- **`overdue`**: a vault has missed two epochs. Requests are stuck until a NAV lands.
- **`updater_mismatch`**: `config.nav_updater` was rotated; deploy the new key or fix the config.
