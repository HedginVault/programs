use anchor_lang::prelude::*;

use crate::{ProtocolStatus, StrategyType, VaultStatus};

// Config

#[event]
pub struct ConfigInitialized {
    pub admin: Pubkey,
    pub nav_updater: Pubkey,
    pub treasury_authority: Pubkey,
    pub guardian: Pubkey,
}

#[event]
pub struct ConfigUpdated {
    pub admin: Pubkey,
    pub nav_updater: Pubkey,
    pub treasury_authority: Pubkey,
    pub guardian: Pubkey,
    pub platform_performance_fee_bps: u16,
    pub platform_management_fee_bps: u16,
    pub max_nav_deviation_bps: u16,
    pub max_epoch_outflow_bps: u16,
    pub max_slippage_bps: u16,
    pub status: ProtocolStatus,
}

#[event]
pub struct ConfigMigrated {
    pub version: u8,
}

#[event]
pub struct AdminNominated {
    pub admin: Pubkey,
    pub pending_admin: Pubkey,
}

#[event]
pub struct AdminAccepted {
    pub previous_admin: Pubkey,
    pub admin: Pubkey,
}

#[event]
pub struct ProtocolPaused {
    pub guardian: Pubkey,
}

#[event]
pub struct ManagerAdded {
    pub authority: Pubkey,
}

#[event]
pub struct ManagerRemoved {
    pub authority: Pubkey,
}

// Vault

#[event]
pub struct VaultInitialized {
    pub vault: Pubkey,
    pub id: u64,
    pub authority: Pubkey,
    pub deposit_mint: Pubkey,
    pub share_mint: Pubkey,
}

#[event]
pub struct VaultUpdated {
    pub vault: Pubkey,
    pub performance_fee_bps: u16,
    pub management_fee_bps: u16,
    pub pending_performance_fee_bps: u16,
    pub pending_management_fee_bps: u16,
    pub fee_effective_ts: i64,
    pub deposit_cap: u64,
    pub min_deposit: u64,
    pub min_withdrawal_shares: u64,
    pub status: VaultStatus,
}

#[event]
pub struct VaultPaused {
    pub vault: Pubkey,
    pub guardian: Pubkey,
}

#[event]
pub struct VaultClosed {
    pub vault: Pubkey,
}

#[event]
pub struct NavUpdated {
    pub vault: Pubkey,
    pub epoch: u64,
    pub total_assets: u64,
    pub nav_per_share: u64,
    pub high_water_mark: u64,
    pub manager_fee_shares: u64,
    pub platform_fee_shares: u64,
    pub overridden: bool,
}

#[event]
pub struct ManagerFeeClaimed {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub shares: u64,
}

#[event]
pub struct PlatformFeeClaimed {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub shares: u64,
}

// Requests

#[event]
pub struct DepositRequested {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
    pub pending_amount: u64,
    pub epoch: u64,
}

#[event]
pub struct DepositCancelled {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
}

#[event]
pub struct DepositResolved {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
    pub shares: u64,
    pub nav_per_share: u64,
}

#[event]
pub struct DepositRejected {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
}

#[event]
pub struct WithdrawalRequested {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub shares: u64,
    pub pending_shares: u64,
    pub epoch: u64,
}

#[event]
pub struct WithdrawalCancelled {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub shares: u64,
}

#[event]
pub struct WithdrawalResolved {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub shares: u64,
    pub amount: u64,
    pub nav_per_share: u64,
}

#[event]
pub struct WithdrawalRejected {
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub shares: u64,
}

// Strategies

#[event]
pub struct StrategyInitialized {
    pub vault: Pubkey,
    pub strategy: Pubkey,
    pub id: u32,
    pub strategy_type: StrategyType,
}

#[event]
pub struct StrategyClosed {
    pub vault: Pubkey,
    pub strategy: Pubkey,
}

#[event]
pub struct JupiterSwapped {
    pub vault: Pubkey,
    pub strategy: Pubkey,
    pub source_mint: Pubkey,
    pub destination_mint: Pubkey,
    pub amount: u64,
}

#[event]
pub struct MeteoraDlmmLiquidityAdded {
    pub vault: Pubkey,
    pub strategy: Pubkey,
    pub position: Pubkey,
    pub amount_x: u64,
    pub amount_y: u64,
}

#[event]
pub struct MeteoraDlmmLiquidityRemoved {
    pub vault: Pubkey,
    pub strategy: Pubkey,
    pub position: Pubkey,
    pub bps_to_remove: u16,
}

#[event]
pub struct MeteoraDlmmFeeClaimed {
    pub vault: Pubkey,
    pub strategy: Pubkey,
    pub position: Pubkey,
    /// Total fees claimed into the vault, including the treasury share.
    pub amount_x: u64,
    pub amount_y: u64,
    pub treasury_amount_x: u64,
    pub treasury_amount_y: u64,
}
