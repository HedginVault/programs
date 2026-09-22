use anchor_lang::prelude::*;

use crate::{error::HedgeVaultError, validate, validate_pda, SafeMathAssign};

pub struct NewWithdrawalRequestArgs {
    pub authority: Pubkey,
    pub vault: Pubkey,
    pub epoch: u64,
    pub created_ts: i64,
    pub bump: u8,
}

/// Pending withdrawal, resolvable once the vault NAV is updated in a later epoch.
#[account]
#[derive(InitSpace)]
pub struct WithdrawalRequest {
    /// Authority of the request, receives the redeemed deposit mint.
    pub authority: Pubkey,
    /// The vault this request is for.
    pub vault: Pubkey,
    /// Shares held in the vault's share escrow.
    pub shares: u64,
    /// Epoch the request was made in.
    pub epoch: u64,
    /// Timestamp the request was created.
    pub created_ts: i64,
    pub bump: u8,
    /// Lamports held above this account's own rent, earmarked for the payout token account created
    /// at settlement. Refunded with the rent when the request closes.
    pub rent_escrow: u64,
    /// Reserved for future fields.
    pub reserved: [u8; 24],
}

impl WithdrawalRequest {
    pub fn new(args: NewWithdrawalRequestArgs) -> Self {
        Self {
            authority: args.authority,
            vault: args.vault,
            shares: 0,
            epoch: args.epoch,
            created_ts: args.created_ts,
            bump: args.bump,
            rent_escrow: 0,
            reserved: [0; 24],
        }
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidRequest.into())
    }

    pub fn validate_authority(&self, authority: Pubkey) -> Result<()> {
        validate!(
            authority == self.authority,
            HedgeVaultError::InvalidRequestAuthority
        )?;

        Ok(())
    }

    pub fn validate_vault(&self, vault: Pubkey) -> Result<()> {
        validate!(vault == self.vault, HedgeVaultError::InvalidRequestVault)?;

        Ok(())
    }

    /// Requests can only accumulate within a single epoch.
    pub fn add(&mut self, shares: u64, epoch: u64) -> Result<()> {
        validate!(
            self.epoch == epoch,
            HedgeVaultError::PendingRequestNotResolved
        )?;

        self.shares.safe_add_assign(shares)
    }

    pub fn is_resolvable(&self, vault_nav_epoch: u64) -> Result<()> {
        validate!(
            vault_nav_epoch > self.epoch,
            HedgeVaultError::RequestNotResolvable
        )?;

        Ok(())
    }

    pub fn is_cancellable(&self, vault_nav_epoch: u64) -> Result<()> {
        validate!(
            vault_nav_epoch <= self.epoch,
            HedgeVaultError::RequestNotCancellable
        )?;

        Ok(())
    }
}
