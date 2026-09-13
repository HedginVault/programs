# Hedge Vault — Account Data Model

Every on-chain account the program owns, read like a database schema. All
accounts are PDAs of the program (`r2ahBQ6gbPCJ9FxBymYcXuwXi8NmenRry7SE7QR7FAt`)
except the SPL token accounts and share mint, which are owned by the token
programs but whose authority is a program PDA.

Sizes include the 8-byte Anchor discriminator. Amounts are raw token units
(no decimals applied). `bps` = basis points, 10 000 = 100 %.

## Entity relationship diagram

```mermaid
erDiagram
    CONFIG ||--o{ MANAGER : "whitelists"
    CONFIG ||--o{ VAULT : "issues vault ids to"
    MANAGER ||--o{ VAULT : "authority creates"
    VAULT ||--|| SHARE_MINT : "mints"
    VAULT ||--|| VAULT_TOKEN_ACCOUNT : "holds idle deposit mint in"
    VAULT ||--|| DEPOSIT_ESCROW : "holds pending deposits in"
    VAULT ||--|| SHARE_ESCROW : "holds pending withdrawal shares in"
    VAULT ||--o{ STRATEGY : "records fund utilization in"
    VAULT ||--o{ DEPOSIT_REQUEST : "queues"
    VAULT ||--o{ WITHDRAWAL_REQUEST : "queues"
    STRATEGY }o--|| PROTOCOL_ACCOUNT : "points to"

    CONFIG {
        Pubkey admin "config + manager whitelist authority"
        Pubkey nav_updater "may call update_nav"
        Pubkey treasury_authority "may claim platform fee shares"
        Pubkey guardian "may only pause"
        u64 next_vault_id "auto-increment"
        u16 platform_performance_fee_bps
        u16 platform_management_fee_bps "annualized"
        u16 max_nav_deviation_bps "per update_nav"
        u16 max_epoch_outflow_bps "per epoch"
        ProtocolStatus status
        u8 bump
    }

    MANAGER {
        Pubkey authority PK "whitelisted vault creator"
        u8 bump
    }

    VAULT {
        u64 id PK "from config.next_vault_id"
        Pubkey authority PK "manager"
        bytes32 name
        bytes64 description
        Pubkey deposit_mint FK
        Pubkey share_mint FK
        u64 deposit_cap
        u64 total_assets "last posted AUM, adjusted on resolve"
        u64 nav_per_share "x1e9"
        u64 high_water_mark "x1e9"
        u64 nav_epoch
        i64 last_nav_ts
        u64 pending_deposits
        u64 pending_withdrawal_shares
        u64 unclaimed_manager_fee_shares
        u64 unclaimed_platform_fee_shares
        u64 epoch_outflow "reset on NAV update"
        u16 performance_fee_bps
        u16 management_fee_bps "annualized"
        VaultStatus status
        u8 next_strategy_id "auto-increment"
        u8 bump
    }

    SHARE_MINT {
        Pubkey mint_authority "= vault"
        u8 decimals "= deposit mint decimals"
        u64 supply "outstanding shares"
    }

    VAULT_TOKEN_ACCOUNT {
        Pubkey owner "= vault (ATA)"
        Pubkey mint "= deposit_mint"
        u64 amount "idle funds, pays withdrawals"
    }

    DEPOSIT_ESCROW {
        Pubkey owner "= vault"
        Pubkey mint "= deposit_mint"
        u64 amount "= vault.pending_deposits"
    }

    SHARE_ESCROW {
        Pubkey owner "= vault"
        Pubkey mint "= share_mint"
        u64 amount "= vault.pending_withdrawal_shares"
    }

    STRATEGY {
        Pubkey vault FK
        Pubkey protocol_account PK "target_mint or position"
        i64 created_ts
        i64 last_action_ts
        u8 id "unique within vault"
        u8 bump
        StrategyType strategy_type "JupiterSwap or MeteoraDlmm"
    }

    PROTOCOL_ACCOUNT {
        Pubkey key "Jupiter target mint or DLMM PositionV2"
    }

    DEPOSIT_REQUEST {
        Pubkey vault FK
        Pubkey authority PK "depositor"
        u64 amount "deposit mint in escrow"
        u64 epoch "epoch requested"
        u8 bump
    }

    WITHDRAWAL_REQUEST {
        Pubkey vault FK
        Pubkey authority PK "withdrawer"
        u64 shares "shares in escrow"
        u64 epoch "epoch requested"
        u8 bump
    }
```

## Tables

### Config (singleton)

| | |
| --- | --- |
| Seeds | `["config"]` |
| Layout | zero-copy, 160 bytes |
| Created by | `initialize_config` (admin = signer) |
| Mutated by | `update_config`, `pause_protocol`, `initialize_vault` (`next_vault_id`) |
| Closed by | never |

| Field | Type | Description |
| --- | --- | --- |
| `admin` | `Pubkey` | Can update config and add/remove managers. |
| `nav_updater` | `Pubkey` | Only signer allowed to call `update_nav`. |
| `treasury_authority` | `Pubkey` | Only signer allowed to call `claim_platform_fee`. |
| `guardian` | `Pubkey` | Only signer allowed to call `pause_protocol`. Cannot unpause or change anything else. |
| `next_vault_id` | `u64` | Incremented on every `initialize_vault`; becomes `vault.id`. |
| `platform_performance_fee_bps` | `u16` | Platform cut of profit above the vault high water mark. |
| `platform_management_fee_bps` | `u16` | Annualized platform fee on total assets, prorated by seconds since last NAV. |
| `max_nav_deviation_bps` | `u16` | `update_nav` is rejected if NAV per share moves more than this from the previous value. `override_nav` (admin) ignores it. |
| `max_epoch_outflow_bps` | `u16` | Withdrawal resolutions fail once payouts since the last NAV update exceed this share of total assets. |
| `status` | `ProtocolStatus` | `Normal` (0), `Paused` (1), `ReduceOnly` (2). Starts `Paused`. |
| `bump` | `u8` | PDA bump. |

Status gates: `Normal` required for deposits, vault creation, strategy execute/exit and deposit resolution. `Paused` blocks withdrawal requests and resolutions too. `update_nav`, `override_nav` and fee claims are never gated.

NAV safety checks, in order: `total_assets >= vault_token_account.amount` (both update paths), one update per epoch, deviation bound (`update_nav` only), fee must be below total assets.

### Manager

| | |
| --- | --- |
| Seeds | `["manager", authority]` |
| Layout | borsh, 41 bytes |
| Created by | `add_manager` (admin) |
| Closed by | `remove_manager` (admin); existing vaults keep working |

| Field | Type | Description |
| --- | --- | --- |
| `authority` | `Pubkey` | Wallet allowed to call `initialize_vault`. |
| `bump` | `u8` | PDA bump. |

### Vault

| | |
| --- | --- |
| Seeds | `["vault", id (u64 LE), authority]` |
| Layout | zero-copy, 424 bytes (120 reserved) |
| Created by | `initialize_vault` (manager) |
| Mutated by | `update_vault`, `update_nav`, `override_nav`, request/cancel/resolve, fee claims, strategy init |
| Closed by | `close_vault` when share supply, pending totals and unclaimed fee shares are all 0 |

| Field | Type | Description |
| --- | --- | --- |
| `id` | `u64` | Sequential id from config. |
| `authority` | `Pubkey` | Manager; signs all strategy and vault management instructions. |
| `name` | `[u8; 32]` | UTF-8, zero padded. |
| `description` | `[u8; 64]` | UTF-8, zero padded. |
| `deposit_mint` | `Pubkey` | Only mint accepted for deposits and paid on withdrawals. |
| `share_mint` | `Pubkey` | Tokenized share mint, see below. |
| `deposit_cap` | `u64` | `total_assets + pending_deposits` must stay below this on `request_deposit`. |
| `total_assets` | `u64` | AUM posted by the updater. `+amount` on deposit resolve, `-amount` on withdrawal resolve so NAV stays constant between updates. |
| `nav_per_share` | `u64` | Value of one share in deposit mint, scaled by 1e9. Starts at 1e9. |
| `high_water_mark` | `u64` | Highest `nav_per_share` after fees. Performance fee only on NAV above it. |
| `nav_epoch` | `u64` | `unix_ts / 86400` of the last NAV update. Requests resolve only if `nav_epoch > request.epoch`. |
| `last_nav_ts` | `i64` | Timestamp of last update, start of the management fee accrual window. |
| `pending_deposits` | `u64` | Mirrors the deposit escrow balance. |
| `pending_withdrawal_shares` | `u64` | Mirrors the share escrow balance. |
| `unclaimed_manager_fee_shares` | `u64` | Shares owed to the manager, counted as supply in NAV math until minted by `claim_manager_fee`. |
| `unclaimed_platform_fee_shares` | `u64` | Same for the platform, minted by `claim_platform_fee`. |
| `epoch_outflow` | `u64` | Deposit mint paid to withdrawals since the last NAV update. Reset to 0 by `update_nav` / `override_nav`. Cap is `(total_assets + epoch_outflow) * max_epoch_outflow_bps / 10_000`. |
| `performance_fee_bps` | `u16` | Manager cut of profit above high water mark. |
| `management_fee_bps` | `u16` | Annualized manager fee on total assets. |
| `status` | `VaultStatus` | `Normal` (0), `Paused` (1), `ReduceOnly` (2). Starts `Normal`. |
| `next_strategy_id` | `u8` | Incremented on every strategy init. |
| `bump` | `u8` | PDA bump. |

Derived value used by `update_nav`: `virtual_supply = share_mint.supply + unclaimed_manager_fee_shares + unclaimed_platform_fee_shares`.

### Vault-owned token accounts

| Account | Seeds / address | Mint | Purpose |
| --- | --- | --- | --- |
| Share mint | `["share_mint", vault]` | — | SPL Token mint, decimals = deposit mint decimals, mint authority = vault, no freeze authority. `supply` = shares held by users plus escrow. |
| Vault token account | ATA(vault, deposit_mint) | deposit mint | Idle funds. Source for strategy deployments and withdrawal payouts. |
| Strategy token accounts | ATA(vault, any mint) | any | Created on demand by strategy execute/exit (Jupiter target mints, DLMM token X/Y). |
| Deposit escrow | `["deposit_escrow", vault]` | deposit mint | Pending deposits. Balance always equals `vault.pending_deposits`. |
| Share escrow | `["share_escrow", vault]` | share mint | Pending withdrawal shares. Balance always equals `vault.pending_withdrawal_shares`. Shares are burned from here on resolve. |

### Strategy

| | |
| --- | --- |
| Seeds | `["strategy", vault, protocol_account]` |
| Layout | borsh, 97 bytes |
| Created by | `initialize_strategy_jupiter_swap`, `initialize_strategy_meteora_dlmm` (manager) |
| Mutated by | execute/exit (`last_action_ts` only) |
| Closed by | `close_strategy`; also closes the protocol account if it is empty |

| Field | Type | Description |
| --- | --- | --- |
| `vault` | `Pubkey` | Parent vault. |
| `created_ts` | `i64` | Creation timestamp. |
| `last_action_ts` | `i64` | Last execute or exit. |
| `id` | `u8` | Sequential within the vault. |
| `bump` | `u8` | PDA bump. |
| `strategy_type` | `StrategyType` | Enum, see below. |

`StrategyType` (borsh, 1 tag byte + 32 bytes):

| Tag | Variant | Payload | `protocol_account` seed |
| --- | --- | --- | --- |
| 0 | `JupiterSwap` | `target_mint: Pubkey` | target mint |
| 1 | `MeteoraDlmm` | `position: Pubkey` | DLMM `PositionV2` owned by the vault |

No amounts are stored. Utilization is read from the protocol accounts and token balances off-chain.

### DepositRequest

| | |
| --- | --- |
| Seeds | `["deposit_request", vault, depositor]` (one per user per vault) |
| Layout | borsh, 89 bytes |
| Created by | `request_deposit` (depositor pays rent) |
| Mutated by | `request_deposit` again in the same epoch (amount accumulates) |
| Closed by | `cancel_deposit_request` (depositor, only while `vault.nav_epoch <= epoch`) or `resolve_deposit_request` (anyone, only when `vault.nav_epoch > epoch`); rent returns to `authority` |

| Field | Type | Description |
| --- | --- | --- |
| `authority` | `Pubkey` | Depositor; receives minted shares in their share ATA. |
| `vault` | `Pubkey` | Target vault. |
| `amount` | `u64` | Deposit mint held in deposit escrow. |
| `epoch` | `u64` | Epoch of the first request. A request from an older epoch must be resolved before a new one is accepted. |
| `bump` | `u8` | PDA bump. |

On resolve: `shares = amount * 1e9 / vault.nav_per_share`.

### WithdrawalRequest

| | |
| --- | --- |
| Seeds | `["withdrawal_request", vault, withdrawer]` (one per user per vault) |
| Layout | borsh, 89 bytes |
| Created by | `request_withdrawal` (withdrawer pays rent) |
| Mutated by | `request_withdrawal` again in the same epoch (shares accumulate) |
| Closed by | `cancel_withdrawal_request` (withdrawer, only while `vault.nav_epoch <= epoch`) or `resolve_withdrawal_request` (anyone, only when `vault.nav_epoch > epoch`); rent returns to `authority` |

| Field | Type | Description |
| --- | --- | --- |
| `authority` | `Pubkey` | Withdrawer; receives deposit mint in their ATA. |
| `vault` | `Pubkey` | Target vault. |
| `shares` | `u64` | Shares held in share escrow. |
| `epoch` | `u64` | Epoch of the first request. |
| `bump` | `u8` | PDA bump. |

On resolve: `amount = shares * vault.nav_per_share / 1e9`, paid from the vault token account, which must hold at least that balance, and subject to the per-epoch outflow cap.

## Request lifecycle

```mermaid
stateDiagram-v2
    [*] --> Pending : request_* (epoch N)
    Pending --> Pending : request_* again in epoch N
    Pending --> [*] : cancel_* while no NAV newer than epoch N
    Pending --> Resolvable : update_nav in a later epoch
    Resolvable --> [*] : resolve_* (anyone)
```

## NAV update per epoch

```mermaid
flowchart TD
    A[update_nav total_assets] --> A2{total_assets at least idle balance?}
    A2 -- no --> X0[error TotalAssetsBelowIdleBalance]
    A2 -- yes --> B{epoch later than vault.nav_epoch?}
    B -- no --> X[error NavAlreadyUpdatedThisEpoch]
    B -- yes --> C[supply = mint.supply + unclaimed fee shares]
    C --> D[gross_nav = total_assets * 1e9 / supply]
    D --> E[mgmt fee = total_assets * bps * elapsed / year]
    D --> F[perf fee = max 0, gross_nav - hwm * supply * bps]
    E --> G[fee shares = fee * supply / total_assets - total_fee]
    F --> G
    G --> H[nav = total_assets * 1e9 / supply + fee shares]
    H --> J{nav within max_nav_deviation_bps of previous?}
    J -- no, update_nav --> X2[error NavDeviationExceeded, admin may override_nav]
    J -- yes or override_nav --> I[store nav, hwm = max, epoch, ts, unclaimed += fee shares, epoch_outflow = 0]
```
