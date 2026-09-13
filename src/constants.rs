use anchor_lang::prelude::*;

#[constant]
pub const MAX_BPS: u16 = 10_000; // 100%

/// Layout versions, bumped whenever an account grows. Zero means the pre-versioned layout.
#[constant]
pub const CONFIG_VERSION: u8 = 2;
#[constant]
pub const VAULT_VERSION: u8 = 1;
#[constant]
pub const STRATEGY_VERSION: u8 = 1;

/// Size of the deployed v1 config account, the only layout `migrate_config` accepts.
pub const CONFIG_V1_LEN: usize = 160;

/// Precision of NAV per share, 1e9 = 1 deposit mint unit per share.
#[constant]
pub const NAV_PRECISION: u64 = 1_000_000_000;

/// Duration of a NAV epoch in seconds. NAV is updated at most once per epoch.
#[constant]
pub const EPOCH_DURATION: i64 = 86_400; // 24h

/// Used to prorate annualized management fees.
#[constant]
pub const SECONDS_PER_YEAR: i64 = 31_536_000; // 365 days
