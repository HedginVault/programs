use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use num_derive::{FromPrimitive, ToPrimitive};

use crate::{
    error::HedgeVaultError, validate, validate_pda, SafeConvert, SafeMath, SafeMathAssign,
    EPOCH_DURATION, MAX_BPS, NAV_PRECISION, SECONDS_PER_YEAR, VAULT_VERSION,
};

pub struct NewVaultArgs {
    pub id: u64,
    pub authority: Pubkey,
    pub name: [u8; 32],
    pub description: [u8; 64],
    pub deposit_mint: Pubkey,
    pub share_mint: Pubkey,
    pub deposit_cap: u64,
    pub performance_fee_bps: u16,
    pub management_fee_bps: u16,
    pub current_ts: i64,
    pub bump: u8,
}

pub struct NavUpdate {
    pub manager_fee_shares: u64,
    pub platform_fee_shares: u64,
}

pub struct UpdateNavArgs {
    /// Total value of vault holdings across all strategies, denoted in deposit mint.
    pub total_assets: u64,
    /// Current supply of the share mint, excluding unclaimed fee shares.
    pub share_supply: u64,
    pub platform_performance_fee_bps: u16,
    pub platform_management_fee_bps: u16,
    /// Max NAV change allowed for this update, `None` skips the check.
    pub max_nav_deviation_bps: Option<u16>,
    pub now: i64,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    FromPrimitive,
    ToPrimitive,
    Clone,
    Copy,
    PartialEq,
)]
#[repr(u8)]
pub enum VaultStatus {
    Normal,
    Paused,
    ReduceOnly,
}

unsafe impl Zeroable for VaultStatus {}
unsafe impl Pod for VaultStatus {}

#[account(zero_copy)]
#[derive(InitSpace)]
pub struct Vault {
    /// Unique vault ID.
    pub id: u64,
    /// Authority allowed to manage the vault.
    pub authority: Pubkey,
    /// Name of the vault (utf8 bytes, padded with 0s).
    pub name: [u8; 32],
    /// Description of the vault (utf8 bytes, padded with 0s).
    pub description: [u8; 64],
    /// Mint accepted as vault deposits.
    pub deposit_mint: Pubkey,
    /// Mint of the tokenized vault shares, authority is the vault.
    pub share_mint: Pubkey,
    /// Max total assets accepted including pending deposits, denoted in deposit mint.
    pub deposit_cap: u64,
    /// Last reported total value of vault holdings, adjusted on request resolution. Denoted in deposit mint.
    pub total_assets: u64,
    /// Last recorded value of one share, scaled by [NAV_PRECISION]. Denoted in deposit mint.
    pub nav_per_share: u64,
    /// All-time highest recorded NAV per share after fees.
    ///
    /// Performance fees are only taken on profits above this mark.
    pub high_water_mark: u64,
    /// Epoch of the last NAV update.
    pub nav_epoch: u64,
    /// Timestamp of the last NAV update, used to prorate management fees.
    pub last_nav_ts: i64,
    /// Total deposit mint held in deposit escrow awaiting resolution.
    pub pending_deposits: u64,
    /// Total shares held in share escrow awaiting resolution.
    pub pending_withdrawal_shares: u64,
    /// Fee shares accrued to the vault manager, minted on claim.
    pub unclaimed_manager_fee_shares: u64,
    /// Fee shares accrued to the platform, minted on claim.
    pub unclaimed_platform_fee_shares: u64,
    /// Deposit mint paid out to withdrawals since the last NAV update.
    pub epoch_outflow: u64,
    /// Next strategy ID, increments with each new strategy.
    pub next_strategy_id: u32,
    /// Fee taken from profits above the high water mark that goes to the vault manager, denoted in basis points.
    pub performance_fee_bps: u16,
    /// Annualized fee taken on total assets that goes to the vault manager, denoted in basis points.
    pub management_fee_bps: u16,
    /// Determines operational status of the vault.
    pub status: VaultStatus,
    pub bump: u8,
    /// Layout version, see [VAULT_VERSION].
    pub version: u8,
    padding0: [u8; 5],
    padding1: [u64; 14],
}

impl Vault {
    pub fn new(args: NewVaultArgs) -> Self {
        Self {
            id: args.id,
            authority: args.authority,
            name: args.name,
            description: args.description,
            deposit_mint: args.deposit_mint,
            share_mint: args.share_mint,
            deposit_cap: args.deposit_cap,
            total_assets: 0,
            nav_per_share: NAV_PRECISION,
            high_water_mark: NAV_PRECISION,
            nav_epoch: 0,
            last_nav_ts: args.current_ts,
            pending_deposits: 0,
            pending_withdrawal_shares: 0,
            unclaimed_manager_fee_shares: 0,
            unclaimed_platform_fee_shares: 0,
            epoch_outflow: 0,
            next_strategy_id: 0,
            performance_fee_bps: args.performance_fee_bps,
            management_fee_bps: args.management_fee_bps,
            status: VaultStatus::Normal,
            bump: args.bump,
            version: VAULT_VERSION,
            padding0: [0; 5],
            padding1: [0; 14],
        }
    }

    pub fn epoch(now: i64) -> u64 {
        (now / EPOCH_DURATION) as u64
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidVault.into())
    }

    pub fn validate_authority(&self, authority: Pubkey) -> Result<()> {
        validate!(
            authority == self.authority,
            HedgeVaultError::InvalidVaultAuthority
        )?;

        Ok(())
    }

    pub fn validate_deposit_mint(&self, deposit_mint: Pubkey) -> Result<()> {
        validate!(
            deposit_mint == self.deposit_mint,
            HedgeVaultError::InvalidDepositMint
        )?;

        Ok(())
    }

    pub fn validate_share_mint(&self, share_mint: Pubkey) -> Result<()> {
        validate!(
            share_mint == self.share_mint,
            HedgeVaultError::InvalidShareMint
        )?;

        Ok(())
    }

    pub fn increment_strategy_id(&mut self) -> Result<()> {
        self.next_strategy_id.safe_add_assign(1)
    }

    pub fn is_vault_operational(self) -> Result<()> {
        validate!(
            self.status == VaultStatus::Normal,
            HedgeVaultError::VaultNotOperational
        )?;

        Ok(())
    }

    pub fn is_vault_withdrawable(self) -> Result<()> {
        validate!(
            self.status != VaultStatus::Paused,
            HedgeVaultError::VaultNotWithdrawable
        )?;

        Ok(())
    }

    // NAV

    /// Settles fees as share dilution and records the new NAV. Callable once per epoch.
    pub fn update_nav(&mut self, args: UpdateNavArgs) -> Result<NavUpdate> {
        let UpdateNavArgs {
            total_assets,
            share_supply,
            platform_performance_fee_bps,
            platform_management_fee_bps,
            max_nav_deviation_bps,
            now,
        } = args;

        let epoch = Self::epoch(now);
        validate!(
            epoch > self.nav_epoch,
            HedgeVaultError::NavAlreadyUpdatedThisEpoch
        )?;

        // unclaimed fee shares are counted as supply so claiming later does not move NAV
        let supply = share_supply
            .safe_add(self.unclaimed_manager_fee_shares)?
            .safe_add(self.unclaimed_platform_fee_shares)? as u128;

        let mut nav_update = NavUpdate {
            manager_fee_shares: 0,
            platform_fee_shares: 0,
        };

        if supply == 0 {
            self.nav_per_share = NAV_PRECISION;
        } else {
            let total_assets = total_assets as u128;
            let elapsed = now.saturating_sub(self.last_nav_ts) as u128;
            let gross_nav = total_assets
                .safe_mul(NAV_PRECISION as u128)?
                .safe_div(supply)?;
            let eligible_profit = if gross_nav > self.high_water_mark as u128 {
                gross_nav
                    .safe_sub(self.high_water_mark as u128)?
                    .safe_mul(supply)?
                    .safe_div(NAV_PRECISION as u128)?
            } else {
                0
            };

            let manager_fee = Self::management_fee(
                total_assets,
                self.management_fee_bps,
                elapsed,
            )?
            .safe_add(Self::performance_fee(
                eligible_profit,
                self.performance_fee_bps,
            )?)?;
            let platform_fee = Self::management_fee(
                total_assets,
                platform_management_fee_bps,
                elapsed,
            )?
            .safe_add(Self::performance_fee(
                eligible_profit,
                platform_performance_fee_bps,
            )?)?;
            let total_fee = manager_fee.safe_add(platform_fee)?;

            validate!(
                total_fee == 0 || total_fee < total_assets,
                HedgeVaultError::FeeExceedsTotalAssets
            )?;

            // fee shares are minted such that they are worth exactly the fee at the new NAV
            let net_assets = total_assets.safe_sub(total_fee)?;
            let (manager_fee_shares, platform_fee_shares) = if total_fee > 0 {
                (
                    manager_fee.safe_mul(supply)?.safe_div(net_assets)?,
                    platform_fee.safe_mul(supply)?.safe_div(net_assets)?,
                )
            } else {
                (0, 0)
            };

            let new_supply = supply
                .safe_add(manager_fee_shares)?
                .safe_add(platform_fee_shares)?;
            let nav_per_share = total_assets
                .safe_mul(NAV_PRECISION as u128)?
                .safe_div(new_supply)?
                .safe_to_u64()?;

            if let Some(max_nav_deviation_bps) = max_nav_deviation_bps {
                self.validate_nav_deviation(nav_per_share, max_nav_deviation_bps)?;
            }

            nav_update.manager_fee_shares = manager_fee_shares.safe_to_u64()?;
            nav_update.platform_fee_shares = platform_fee_shares.safe_to_u64()?;

            self.unclaimed_manager_fee_shares
                .safe_add_assign(nav_update.manager_fee_shares)?;
            self.unclaimed_platform_fee_shares
                .safe_add_assign(nav_update.platform_fee_shares)?;
            self.nav_per_share = nav_per_share;

            if nav_per_share > self.high_water_mark {
                self.high_water_mark = nav_per_share;
            }
        }

        self.total_assets = total_assets;
        self.nav_epoch = epoch;
        self.last_nav_ts = now;
        self.epoch_outflow = 0;

        Ok(nav_update)
    }

    fn validate_nav_deviation(&self, nav_per_share: u64, max_nav_deviation_bps: u16) -> Result<()> {
        let deviation = nav_per_share.abs_diff(self.nav_per_share) as u128;
        let max_deviation = (self.nav_per_share as u128)
            .safe_mul(max_nav_deviation_bps as u128)?
            .safe_div(MAX_BPS as u128)?;

        validate!(
            deviation <= max_deviation,
            HedgeVaultError::NavDeviationExceeded
        )?;

        Ok(())
    }

    fn management_fee(total_assets: u128, fee_bps: u16, elapsed: u128) -> Result<u128> {
        total_assets
            .safe_mul(fee_bps as u128)?
            .safe_mul(elapsed)?
            .safe_div((SECONDS_PER_YEAR as u128).safe_mul(MAX_BPS as u128)?)
    }

    fn performance_fee(eligible_profit: u128, fee_bps: u16) -> Result<u128> {
        eligible_profit
            .safe_mul(fee_bps as u128)?
            .safe_div(MAX_BPS as u128)
    }

    // Requests

    pub fn request_deposit(&mut self, amount: u64) -> Result<()> {
        self.pending_deposits.safe_add_assign(amount)?;

        validate!(
            self.total_assets.safe_add(self.pending_deposits)? <= self.deposit_cap,
            HedgeVaultError::DepositCapReached
        )?;

        Ok(())
    }

    pub fn cancel_deposit(&mut self, amount: u64) -> Result<()> {
        self.pending_deposits.safe_sub_assign(amount)
    }

    /// Returns shares to mint for a resolved deposit at the current NAV.
    pub fn resolve_deposit(&mut self, amount: u64) -> Result<u64> {
        self.pending_deposits.safe_sub_assign(amount)?;
        self.total_assets.safe_add_assign(amount)?;

        // a vault whose holdings went to zero restarts at 1 share per deposit mint unit
        let shares = if self.nav_per_share == 0 {
            amount
        } else {
            (amount as u128)
                .safe_mul(NAV_PRECISION as u128)?
                .safe_div(self.nav_per_share as u128)?
                .safe_to_u64()?
        };

        Ok(shares)
    }

    pub fn request_withdrawal(&mut self, shares: u64) -> Result<()> {
        self.pending_withdrawal_shares.safe_add_assign(shares)
    }

    pub fn cancel_withdrawal(&mut self, shares: u64) -> Result<()> {
        self.pending_withdrawal_shares.safe_sub_assign(shares)
    }

    /// Returns deposit mint amount to transfer for a resolved withdrawal at the current NAV.
    pub fn resolve_withdrawal(&mut self, shares: u64, max_epoch_outflow_bps: u16) -> Result<u64> {
        self.pending_withdrawal_shares.safe_sub_assign(shares)?;

        let amount = (shares as u128)
            .safe_mul(self.nav_per_share as u128)?
            .safe_div(NAV_PRECISION as u128)?
            .safe_to_u64()?;

        let max_epoch_outflow = (self.total_assets as u128)
            .safe_add(self.epoch_outflow as u128)?
            .safe_mul(max_epoch_outflow_bps as u128)?
            .safe_div(MAX_BPS as u128)?;

        self.epoch_outflow.safe_add_assign(amount)?;

        validate!(
            self.epoch_outflow as u128 <= max_epoch_outflow,
            HedgeVaultError::EpochOutflowCapReached
        )?;

        self.total_assets.safe_sub_assign(amount)?;

        Ok(amount)
    }

    // Fees

    pub fn claim_manager_fee(&mut self) -> Result<u64> {
        let shares = self.unclaimed_manager_fee_shares;
        validate!(shares > 0, HedgeVaultError::NoFeeToClaim)?;

        self.unclaimed_manager_fee_shares = 0;

        Ok(shares)
    }

    pub fn claim_platform_fee(&mut self) -> Result<u64> {
        let shares = self.unclaimed_platform_fee_shares;
        validate!(shares > 0, HedgeVaultError::NoFeeToClaim)?;

        self.unclaimed_platform_fee_shares = 0;

        Ok(shares)
    }
}
