use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use num_derive::{FromPrimitive, ToPrimitive};

use crate::{
    error::HedgeVaultError, validate, validate_pda, SafeConvert, SafeMath, SafeMathAssign,
    EPOCH_DURATION, FEE_INCREASE_DELAY, MAX_BPS, NAV_PRECISION, SECONDS_PER_YEAR, VAULT_VERSION,
};

pub struct NewVaultArgs {
    pub id: u64,
    pub authority: Pubkey,
    pub name: [u8; 32],
    pub deposit_mint: Pubkey,
    pub share_mint: Pubkey,
    pub deposit_cap: u64,
    pub min_deposit: u64,
    pub min_withdrawal_shares: u64,
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
    /// 80..144, reserved for two future `Pubkey` fields (e.g. `pending_authority`, `delegate`).
    /// Vault metadata beyond `name` lives off-chain.
    reserved_keys: [u8; 64],
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
    /// 312..320, reserved for `epoch_duration`.
    reserved0: [u8; 8],
    /// Smallest deposit accepted per request, denoted in deposit mint. Zero disables the check.
    pub min_deposit: u64,
    /// Smallest shares accepted per withdrawal request unless it is the withdrawer's full balance. Zero disables the check.
    pub min_withdrawal_shares: u64,
    /// 336..360, reserved for `last_override_ts`, `total_deposited`, `total_withdrawn`.
    reserved1: [u8; 24],
    /// Timestamp from which the pending fees apply, zero when no fee change is pending.
    pub fee_effective_ts: i64,
    /// 368..380, reserved for `epoch_inflow` and per-vault deviation/outflow bps.
    reserved2: [u8; 12],
    /// Performance fee that replaces [Vault::performance_fee_bps] once [Vault::fee_effective_ts] has passed.
    pub pending_performance_fee_bps: u16,
    /// Management fee that replaces [Vault::management_fee_bps] once [Vault::fee_effective_ts] has passed.
    pub pending_management_fee_bps: u16,
    /// Strategies currently open on the vault, must be zero before the vault can be closed.
    pub open_strategy_count: u32,
    /// 388..424, reserved for `nav_update_count` and future fields.
    reserved3: [u8; 36],
}

impl Vault {
    pub fn new(args: NewVaultArgs) -> Self {
        Self {
            id: args.id,
            authority: args.authority,
            name: args.name,
            reserved_keys: [0; 64],
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
            reserved0: [0; 8],
            min_deposit: args.min_deposit,
            min_withdrawal_shares: args.min_withdrawal_shares,
            reserved1: [0; 24],
            fee_effective_ts: 0,
            reserved2: [0; 12],
            pending_performance_fee_bps: 0,
            pending_management_fee_bps: 0,
            open_strategy_count: 0,
            reserved3: [0; 36],
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

    pub fn increment_open_strategies(&mut self) -> Result<()> {
        self.open_strategy_count.safe_add_assign(1)
    }

    pub fn decrement_open_strategies(&mut self) -> Result<()> {
        self.open_strategy_count.safe_sub_assign(1)
    }

    pub fn pause(&mut self) {
        self.status = VaultStatus::Paused;
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

        // after fee accrual, so the period that just ended is charged at the old rate
        self.apply_pending_fees(now);

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
        validate!(self.nav_per_share > 0, HedgeVaultError::VaultNavIsZero)?;
        validate!(
            amount >= self.min_deposit,
            HedgeVaultError::DepositBelowMinimum
        )?;

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
        // a zero NAV means existing shares are worthless, minting against them would hand the deposit to old holders
        validate!(self.nav_per_share > 0, HedgeVaultError::VaultNavIsZero)?;

        let shares = (amount as u128)
            .safe_mul(NAV_PRECISION as u128)?
            .safe_div(self.nav_per_share as u128)?
            .safe_to_u64()?;
        validate!(shares > 0, HedgeVaultError::ZeroSharesMinted)?;

        self.pending_deposits.safe_sub_assign(amount)?;
        self.total_assets.safe_add_assign(amount)?;

        Ok(shares)
    }

    pub fn request_withdrawal(&mut self, shares: u64, share_balance: u64) -> Result<()> {
        // a holder below the minimum can still exit with their full balance
        validate!(
            shares >= self.min_withdrawal_shares || shares == share_balance,
            HedgeVaultError::WithdrawalBelowMinimum
        )?;

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

    /// Fee decreases apply immediately. If either resulting fee is higher than the live one,
    /// the pair is scheduled and applied by the first NAV update after [FEE_INCREASE_DELAY].
    pub fn update_fees(
        &mut self,
        performance_fee_bps: Option<u16>,
        management_fee_bps: Option<u16>,
        now: i64,
    ) -> Result<()> {
        // an omitted fee keeps its latest requested value
        let (current_performance_fee_bps, current_management_fee_bps) = if self.fee_effective_ts == 0 {
            (self.performance_fee_bps, self.management_fee_bps)
        } else {
            (self.pending_performance_fee_bps, self.pending_management_fee_bps)
        };
        let performance_fee_bps = performance_fee_bps.unwrap_or(current_performance_fee_bps);
        let management_fee_bps = management_fee_bps.unwrap_or(current_management_fee_bps);

        validate!(
            performance_fee_bps <= MAX_BPS && management_fee_bps <= MAX_BPS,
            HedgeVaultError::InvalidBasisPoints
        )?;

        if performance_fee_bps <= self.performance_fee_bps
            && management_fee_bps <= self.management_fee_bps
        {
            self.performance_fee_bps = performance_fee_bps;
            self.management_fee_bps = management_fee_bps;
            self.pending_performance_fee_bps = 0;
            self.pending_management_fee_bps = 0;
            self.fee_effective_ts = 0;
        } else {
            self.pending_performance_fee_bps = performance_fee_bps;
            self.pending_management_fee_bps = management_fee_bps;
            self.fee_effective_ts = now.safe_add(FEE_INCREASE_DELAY)?;
        }

        Ok(())
    }

    pub fn apply_pending_fees(&mut self, now: i64) {
        if self.fee_effective_ts != 0 && now >= self.fee_effective_ts {
            self.performance_fee_bps = self.pending_performance_fee_bps;
            self.management_fee_bps = self.pending_management_fee_bps;
            self.pending_performance_fee_bps = 0;
            self.pending_management_fee_bps = 0;
            self.fee_effective_ts = 0;
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const USDC: u64 = 1_000_000;

    fn new_vault(performance_fee_bps: u16, management_fee_bps: u16) -> Vault {
        Vault::new(NewVaultArgs {
            id: 0,
            authority: Pubkey::default(),
            name: [0; 32],
            deposit_mint: Pubkey::default(),
            share_mint: Pubkey::default(),
            deposit_cap: u64::MAX,
            min_deposit: 0,
            min_withdrawal_shares: 0,
            performance_fee_bps,
            management_fee_bps,
            current_ts: 0,
            bump: 0,
        })
    }

    fn nav_args(total_assets: u64, share_supply: u64, now: i64) -> UpdateNavArgs {
        UpdateNavArgs {
            total_assets,
            share_supply,
            platform_performance_fee_bps: 0,
            platform_management_fee_bps: 0,
            max_nav_deviation_bps: None,
            now,
        }
    }

    fn assert_err<T: core::fmt::Debug>(result: Result<T>, error: HedgeVaultError) {
        assert_eq!(result.unwrap_err(), anchor_lang::error::Error::from(error));
    }

    #[test]
    fn layout_size_is_stable() {
        assert_eq!(core::mem::size_of::<Vault>(), 416);

        // account offsets in docs/architecture-evolution.md 3.5 include the 8-byte discriminator
        assert_eq!(core::mem::offset_of!(Vault, reserved_keys) + 8, 80);
        assert_eq!(core::mem::offset_of!(Vault, min_deposit) + 8, 320);
        assert_eq!(core::mem::offset_of!(Vault, min_withdrawal_shares) + 8, 328);
        assert_eq!(core::mem::offset_of!(Vault, fee_effective_ts) + 8, 360);
        assert_eq!(core::mem::offset_of!(Vault, pending_performance_fee_bps) + 8, 380);
        assert_eq!(core::mem::offset_of!(Vault, pending_management_fee_bps) + 8, 382);
        assert_eq!(core::mem::offset_of!(Vault, open_strategy_count) + 8, 384);
    }

    #[test]
    fn open_strategy_count_tracks_initialize_and_close() {
        let mut v = new_vault(0, 0);
        assert_eq!(v.open_strategy_count, 0);

        v.increment_open_strategies().unwrap();
        v.increment_open_strategies().unwrap();
        assert_eq!(v.open_strategy_count, 2);

        v.decrement_open_strategies().unwrap();
        assert_eq!(v.open_strategy_count, 1);

        v.decrement_open_strategies().unwrap();
        assert_err(v.decrement_open_strategies(), HedgeVaultError::MathOverflow);
    }

    #[test]
    fn update_nav_with_no_supply_resets_nav() {
        let mut v = new_vault(0, 0);
        v.update_nav(nav_args(0, 0, DAY)).unwrap();

        assert_eq!(v.nav_per_share, NAV_PRECISION);
        assert_eq!(v.nav_epoch, 1);
        assert_eq!(v.last_nav_ts, DAY);
    }

    #[test]
    fn update_nav_is_limited_to_once_per_epoch() {
        let mut v = new_vault(0, 0);
        v.update_nav(nav_args(0, 0, DAY)).unwrap();

        assert_err(
            v.update_nav(nav_args(0, 0, DAY + 1)).map(|_| ()),
            HedgeVaultError::NavAlreadyUpdatedThisEpoch,
        );
    }

    #[test]
    fn profit_above_high_water_mark_accrues_performance_fee() {
        let mut v = new_vault(2_000, 0);
        v.update_nav(nav_args(0, 0, DAY)).unwrap();

        let update = v.update_nav(nav_args(110 * USDC, 100 * USDC, 2 * DAY)).unwrap();

        // profit 10 USDC, 20% fee = 2 USDC, minted as 2 * 100 / 108 shares
        assert_eq!(update.manager_fee_shares, 1_851_851);
        assert_eq!(update.platform_fee_shares, 0);
        assert_eq!(v.unclaimed_manager_fee_shares, 1_851_851);
        assert_eq!(v.nav_per_share, 1_080_000_009);
        assert_eq!(v.high_water_mark, 1_080_000_009);
    }

    #[test]
    fn management_fee_prorates_over_a_year() {
        let mut v = new_vault(0, 200);
        v.update_nav(nav_args(0, 0, DAY)).unwrap();

        let update = v
            .update_nav(nav_args(100 * USDC, 100 * USDC, DAY + SECONDS_PER_YEAR))
            .unwrap();

        // 2% of 100 USDC = 2 USDC, minted as 2 * 100 / 98 shares
        assert_eq!(update.manager_fee_shares, 2_040_816);
        assert_eq!(v.nav_per_share, 980_000_003);
        assert_eq!(v.high_water_mark, NAV_PRECISION);
    }

    #[test]
    fn nav_deviation_bound_rejects_large_moves() {
        let mut v = new_vault(0, 0);
        v.update_nav(nav_args(0, 0, DAY)).unwrap();

        let mut args = nav_args(120 * USDC, 100 * USDC, 2 * DAY);
        args.max_nav_deviation_bps = Some(1_000);

        assert_err(
            v.update_nav(args).map(|_| ()),
            HedgeVaultError::NavDeviationExceeded,
        );
    }

    #[test]
    fn request_deposit_enforces_cap() {
        let mut v = new_vault(0, 0);
        v.deposit_cap = 100 * USDC;

        v.request_deposit(60 * USDC).unwrap();
        assert_err(
            v.request_deposit(50 * USDC),
            HedgeVaultError::DepositCapReached,
        );
    }

    #[test]
    fn resolve_deposit_mints_at_current_nav() {
        let mut v = new_vault(0, 0);
        v.nav_per_share = 2 * NAV_PRECISION;
        v.pending_deposits = 50 * USDC;

        let shares = v.resolve_deposit(50 * USDC).unwrap();

        assert_eq!(shares, 25 * USDC);
        assert_eq!(v.pending_deposits, 0);
        assert_eq!(v.total_assets, 50 * USDC);
    }

    #[test]
    fn resolve_withdrawal_enforces_epoch_outflow_cap() {
        let mut v = new_vault(0, 0);
        v.total_assets = 100 * USDC;
        v.pending_withdrawal_shares = 55 * USDC;

        assert_eq!(v.resolve_withdrawal(30 * USDC, 5_000).unwrap(), 30 * USDC);
        assert_err(
            v.resolve_withdrawal(25 * USDC, 5_000),
            HedgeVaultError::EpochOutflowCapReached,
        );
    }

    #[test]
    fn request_deposit_rejected_when_nav_is_zero() {
        let mut v = new_vault(0, 0);
        v.nav_per_share = 0;

        assert_err(v.request_deposit(10 * USDC), HedgeVaultError::VaultNavIsZero);
    }

    #[test]
    fn resolve_deposit_rejected_when_nav_is_zero() {
        let mut v = new_vault(0, 0);
        v.nav_per_share = 0;
        v.pending_deposits = 10 * USDC;

        assert_err(v.resolve_deposit(10 * USDC), HedgeVaultError::VaultNavIsZero);
    }

    #[test]
    fn resolve_deposit_rejects_zero_shares() {
        let mut v = new_vault(0, 0);
        v.nav_per_share = 2 * NAV_PRECISION;
        v.pending_deposits = 1;

        assert_err(v.resolve_deposit(1), HedgeVaultError::ZeroSharesMinted);
    }

    #[test]
    fn request_deposit_enforces_minimum() {
        let mut v = new_vault(0, 0);
        v.min_deposit = 10 * USDC;

        assert_err(v.request_deposit(USDC), HedgeVaultError::DepositBelowMinimum);
        v.request_deposit(10 * USDC).unwrap();
    }

    #[test]
    fn request_withdrawal_enforces_minimum_unless_full_balance() {
        let mut v = new_vault(0, 0);
        v.min_withdrawal_shares = 50 * USDC;

        assert_err(
            v.request_withdrawal(40 * USDC, 100 * USDC),
            HedgeVaultError::WithdrawalBelowMinimum,
        );
        v.request_withdrawal(40 * USDC, 40 * USDC).unwrap();
        v.request_withdrawal(50 * USDC, 100 * USDC).unwrap();

        assert_eq!(v.pending_withdrawal_shares, 90 * USDC);
    }

    #[test]
    fn fee_decrease_applies_immediately() {
        let mut v = new_vault(2_000, 200);
        v.update_fees(Some(1_000), None, DAY).unwrap();

        assert_eq!(v.performance_fee_bps, 1_000);
        assert_eq!(v.management_fee_bps, 200);
        assert_eq!(v.fee_effective_ts, 0);
    }

    #[test]
    fn fee_increase_is_scheduled() {
        let mut v = new_vault(1_000, 200);
        v.update_fees(Some(2_000), None, DAY).unwrap();

        assert_eq!(v.performance_fee_bps, 1_000);
        assert_eq!(v.pending_performance_fee_bps, 2_000);
        assert_eq!(v.pending_management_fee_bps, 200);
        assert_eq!(v.fee_effective_ts, DAY + FEE_INCREASE_DELAY);
    }

    #[test]
    fn decrease_cancels_a_pending_increase() {
        let mut v = new_vault(1_000, 0);
        v.update_fees(Some(2_000), None, 0).unwrap();
        v.update_fees(Some(1_000), None, DAY).unwrap();

        assert_eq!(v.performance_fee_bps, 1_000);
        assert_eq!(v.fee_effective_ts, 0);
    }

    #[test]
    fn update_fees_rejects_invalid_bps() {
        let mut v = new_vault(0, 0);

        assert_err(
            v.update_fees(Some(MAX_BPS + 1), None, 0),
            HedgeVaultError::InvalidBasisPoints,
        );
    }

    #[test]
    fn nav_update_applies_pending_fee_after_delay_at_the_old_rate() {
        let mut v = new_vault(0, 0);
        v.update_fees(None, Some(1_000), 0).unwrap();

        v.update_nav(nav_args(0, 0, DAY)).unwrap();
        assert_eq!(v.management_fee_bps, 0);

        let update = v
            .update_nav(nav_args(100 * USDC, 100 * USDC, FEE_INCREASE_DELAY))
            .unwrap();

        // the period that just ended accrued at the old 0 bps rate
        assert_eq!(update.manager_fee_shares, 0);
        assert_eq!(v.management_fee_bps, 1_000);
        assert_eq!(v.fee_effective_ts, 0);
    }
}
