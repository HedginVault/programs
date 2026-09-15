# Hedge Vault — NAV Keeper

Backend service that values every `hedge_vault` vault once per 24 h epoch and posts `nav_update`
as the protocol's `nav_updater`. No UI. Every run is recorded in Postgres.

## What it does

Every `TICK_INTERVAL_SECS` the keeper:

1. Reads `Config`, checks its key is still the `nav_updater`, checks its SOL balance, and skips the
   tick if the protocol is paused (unless `POST_WHILE_PAUSED=true`).
2. Reads every vault. A vault is due when its `nav_epoch` is behind the current epoch
   (`unix_ts / 86400`) and the epoch started more than `EPOCH_POST_OFFSET_SECS` ago.
3. For each due vault, values it, simulates `nav_update(total_assets)`, and sends it only on a
   clean simulation. The outcome is upserted into `nav_runs` and alerted on status change.

### What counts as an asset

Only what the vault's `Strategy` records point to, plus the idle balance:

| Holding | Source | Rule |
| --- | --- | --- |
| Idle | vault ATA for the deposit mint | face value |
| Jupiter strategy | vault ATA for `target_mint` | price(target) / price(deposit) via Jupiter |
| DLMM strategy | position in the strategy | X and Y amounts in full, pending fees at 90 % (treasury takes 10 % on claim) |

Tokens sent to the vault out of band are never counted, so they cannot move NAV. A missing price
for any counted mint aborts the vault's run; the keeper never posts a partial valuation.

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
(`ALERT_INFO=true` adds successful posts). Reasons: `needs_override`, `run_failed`, `overdue`
(≥ 2 epochs behind), `protocol_paused`, `low_sol`, `updater_mismatch`, `posted`. An alert fires
when a vault's status or error changes, not on every retry.

## Run

```sh
cd keeper
yarn install
yarn sync-idl            # after every `anchor build`
cp .env.example .env     # fill RPC_URL, KEEPER_KEYPAIR, DATABASE_URL
yarn dev                 # tsx, or: yarn build && yarn start
```

`KEEPER_KEYPAIR` is the JSON array of 64 numbers from a Solana keypair file, e.g.
`KEEPER_KEYPAIR=$(cat ~/.config/solana/updater.json)`.

Start with `DRY_RUN=true`: the keeper simulates every due vault and writes `dry_run` rows without
sending anything. Flip it off once the breakdowns look right.

Docker:

```sh
docker build -t hedge-keeper keeper
docker run --env-file keeper/.env hedge-keeper
```

Migrations in `migrations/` apply automatically at startup.

## Environment

| Variable | Default | Purpose |
| --- | --- | --- |
| `RPC_URL` | required | reads, simulation, send |
| `KEEPER_KEYPAIR` | required | JSON array of 64 u8, the `nav_updater` secret key |
| `DATABASE_URL` | required | Postgres connection string |
| `TICK_INTERVAL_SECS` | 60 | loop period |
| `EPOCH_POST_OFFSET_SECS` | 300 | wait after an epoch boundary before posting |
| `MIN_SOL_BALANCE` | 0.05 | `low_sol` threshold |
| `POST_WHILE_PAUSED` | false | post even when the protocol is paused |
| `DRY_RUN` | false | simulate and record only |
| `ALERT_WEBHOOK_URL` | unset | JSON POST target |
| `ALERT_INFO` | false | also webhook successful posts |
| `JUPITER_API_HOST` | lite-api.jup.ag (api.jup.ag with a key) | price API |
| `JUPITER_API_KEY` | unset | `x-api-key` |
| `PROGRAM_ID` | from IDL | devnet override |

## Layout

```
src/
  index.ts        startup checks + tick loop
  config.ts       env → KeeperConfig
  chain.ts        Connection, Program, PDAs, batched fetches
  scheduler.ts    pure due/overdue decision
  runner.ts       value → post → record → alert for one vault
  post.ts         build, simulate, classify, sign, send, confirm
  valuation/      pure computeValuation, holdings collection, Pricer (Jupiter), DLMM reader
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
- **`failed` with `position_missing:<pubkey>`**: an open DLMM strategy points at a closed position;
  the manager should `vault_close_strategy` it.
- **`overdue`**: a vault has missed two epochs. Requests are stuck until a NAV lands.
- **`updater_mismatch`**: `config.nav_updater` was rotated; deploy the new key or fix the config.
