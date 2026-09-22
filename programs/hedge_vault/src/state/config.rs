use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use num_derive::{FromPrimitive, ToPrimitive};

use crate::{
    error::HedgeVaultError, validate, validate_pda, SafeMathAssign, CONFIG_VERSION,
    DEFAULT_MAX_SLIPPAGE_BPS,
};

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
pub enum ProtocolStatus {
    Normal,
    Paused,
    ReduceOnly,
}

unsafe impl Zeroable for ProtocolStatus {}
unsafe impl Pod for ProtocolStatus {}

pub struct NewConfigArgs {
    pub admin: Pubkey,
    pub nav_updater: Pubkey,
    pub treasury_authority: Pubkey,
    pub guardian: Pubkey,
    pub platform_performance_fee_bps: u16,
    pub platform_management_fee_bps: u16,
    pub max_nav_deviation_bps: u16,
    pub max_epoch_outflow_bps: u16,
    pub max_slippage_bps: u16,
    pub bump: u8,
}

#[account(zero_copy)]
#[derive(InitSpace)]
pub struct Config {
    /// Authority allowed to update config parameters and manage the manager whitelist.
    pub admin: Pubkey,
    /// Authority allowed to post vault NAV updates.
    pub nav_updater: Pubkey,
    /// Authority allowed to claim platform fee shares.
    pub treasury_authority: Pubkey,
    /// Authority allowed to pause the protocol, and nothing else.
    pub guardian: Pubkey,
    /// Vault ID, increments with each new vault.
    pub next_vault_id: u64,
    /// Fee taken from profits above the high water mark that goes to the platform, denoted in basis points.
    pub platform_performance_fee_bps: u16,
    /// Annualized fee taken on total assets that goes to the platform, denoted in basis points.
    pub platform_management_fee_bps: u16,
    /// Max NAV per share change accepted from the updater in a single update, denoted in basis points.
    pub max_nav_deviation_bps: u16,
    /// Max share of total assets that withdrawals can pay out per epoch, denoted in basis points.
    pub max_epoch_outflow_bps: u16,
    /// Determines operational status of the protocol.
    pub status: ProtocolStatus,
    pub bump: u8,
    padding0: [u8; 6],
    /// Layout version, see [CONFIG_VERSION].
    pub version: u8,
    padding1: [u8; 7],
    /// Admin nominated through `config_update`, becomes admin once it signs `admin_accept`. Default pubkey when none.
    pub pending_admin: Pubkey,
    /// Max slippage accepted on a routed swap, denoted in basis points. Zero means [DEFAULT_MAX_SLIPPAGE_BPS].
    pub max_slippage_bps: u16,
    padding2: [u8; 6],
    reserve: [u64; 18],
}

impl Config {
    pub fn new(args: NewConfigArgs) -> Self {
        Self {
            admin: args.admin,
            nav_updater: args.nav_updater,
            treasury_authority: args.treasury_authority,
            guardian: args.guardian,
            next_vault_id: 0,
            platform_performance_fee_bps: args.platform_performance_fee_bps,
            platform_management_fee_bps: args.platform_management_fee_bps,
            max_nav_deviation_bps: args.max_nav_deviation_bps,
            max_epoch_outflow_bps: args.max_epoch_outflow_bps,
            status: ProtocolStatus::Paused,
            bump: args.bump,
            padding0: [0; 6],
            version: CONFIG_VERSION,
            padding1: [0; 7],
            pending_admin: Pubkey::default(),
            max_slippage_bps: args.max_slippage_bps,
            padding2: [0; 6],
            reserve: [0; 18],
        }
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidConfig.into())
    }

    /// Configs written before the field existed carry a zeroed reserve, so zero falls back
    /// to [DEFAULT_MAX_SLIPPAGE_BPS].
    pub fn max_slippage_bps(&self) -> u16 {
        if self.max_slippage_bps == 0 {
            DEFAULT_MAX_SLIPPAGE_BPS
        } else {
            self.max_slippage_bps
        }
    }

    pub fn validate_admin(&self, admin: Pubkey) -> Result<()> {
        validate!(self.admin == admin, HedgeVaultError::InvalidAdmin)?;

        Ok(())
    }

    pub fn validate_nav_updater(&self, nav_updater: Pubkey) -> Result<()> {
        validate!(
            self.nav_updater == nav_updater,
            HedgeVaultError::InvalidNavUpdater
        )?;

        Ok(())
    }

    pub fn validate_treasury_authority(&self, authority: Pubkey) -> Result<()> {
        validate!(
            self.treasury_authority == authority,
            HedgeVaultError::InvalidTreasuryAuthority
        )?;

        Ok(())
    }

    pub fn validate_guardian(&self, guardian: Pubkey) -> Result<()> {
        validate!(self.guardian == guardian, HedgeVaultError::InvalidGuardian)?;

        Ok(())
    }

    pub fn nominate_admin(&mut self, pending_admin: Pubkey) {
        self.pending_admin = pending_admin;
    }

    pub fn accept_admin(&mut self, signer: Pubkey) -> Result<()> {
        validate!(
            self.pending_admin != Pubkey::default() && self.pending_admin == signer,
            HedgeVaultError::InvalidPendingAdmin
        )?;

        self.admin = signer;
        self.pending_admin = Pubkey::default();

        Ok(())
    }

    pub fn pause(&mut self) {
        self.status = ProtocolStatus::Paused;
    }

    pub fn migrate(&mut self) {
        self.version = CONFIG_VERSION;
    }

    pub fn increment_vault_id(&mut self) -> Result<()> {
        self.next_vault_id.safe_add_assign(1)?;

        Ok(())
    }

    pub fn is_protocol_operational(self) -> Result<()> {
        validate!(
            self.status == ProtocolStatus::Normal,
            HedgeVaultError::ProtocolNotOperational
        )?;

        Ok(())
    }

    pub fn is_protocol_withdrawable(self) -> Result<()> {
        validate!(
            self.status != ProtocolStatus::Paused,
            HedgeVaultError::ProtocolNotWithdrawable
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_config(admin: Pubkey) -> Config {
        Config::new(NewConfigArgs {
            admin,
            nav_updater: admin,
            treasury_authority: admin,
            guardian: admin,
            platform_performance_fee_bps: 0,
            platform_management_fee_bps: 0,
            max_nav_deviation_bps: 0,
            max_epoch_outflow_bps: 0,
            max_slippage_bps: 0,
            bump: 0,
        })
    }

    fn assert_err<T: core::fmt::Debug>(result: Result<T>, error: HedgeVaultError) {
        assert_eq!(result.unwrap_err(), anchor_lang::error::Error::from(error));
    }

    #[test]
    fn layout_size_is_stable() {
        assert_eq!(core::mem::size_of::<Config>(), 344);

        // account offset in docs/architecture-evolution.md 3.4 includes the 8-byte discriminator
        assert_eq!(core::mem::offset_of!(Config, pending_admin) + 8, 168);
        assert_eq!(core::mem::offset_of!(Config, max_slippage_bps) + 8, 200);
    }

    #[test]
    fn max_slippage_bps_falls_back_to_the_default_when_unset() {
        let mut config = new_config(Pubkey::new_unique());
        assert_eq!(config.max_slippage_bps(), DEFAULT_MAX_SLIPPAGE_BPS);

        config.max_slippage_bps = 100;
        assert_eq!(config.max_slippage_bps(), 100);
    }

    #[test]
    fn accept_admin_requires_a_nomination() {
        let admin = Pubkey::new_unique();
        let mut config = new_config(admin);

        assert_err(config.accept_admin(admin), HedgeVaultError::InvalidPendingAdmin);
    }

    #[test]
    fn accept_admin_rejects_other_signers() {
        let mut config = new_config(Pubkey::new_unique());
        config.nominate_admin(Pubkey::new_unique());

        assert_err(
            config.accept_admin(Pubkey::new_unique()),
            HedgeVaultError::InvalidPendingAdmin,
        );
    }

    #[test]
    fn accept_admin_transfers_and_clears_the_nomination() {
        let mut config = new_config(Pubkey::new_unique());
        let new_admin = Pubkey::new_unique();
        config.nominate_admin(new_admin);

        config.accept_admin(new_admin).unwrap();

        assert_eq!(config.admin, new_admin);
        assert_eq!(config.pending_admin, Pubkey::default());
    }
}
