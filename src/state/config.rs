use anchor_lang::prelude::*;
use bytemuck::{Pod, Zeroable};
use num_derive::{FromPrimitive, ToPrimitive};

use crate::{error::HedgeVaultError, validate, validate_pda, SafeMathAssign, CONFIG_VERSION};

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
    reserve: [u64; 23],
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
            reserve: [0; 23],
        }
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidConfig.into())
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
