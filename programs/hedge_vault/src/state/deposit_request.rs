use anchor_lang::prelude::*;

use crate::{error::HedgeVaultError, validate, validate_pda, SafeMathAssign};

pub struct NewDepositRequestArgs {
    pub authority: Pubkey,
    pub vault: Pubkey,
    pub epoch: u64,
    pub bump: u8,
}

/// Pending deposit, resolvable once the vault NAV is updated in a later epoch.
#[account]
#[derive(InitSpace)]
pub struct DepositRequest {
    /// Authority of the request, receives the minted shares.
    pub authority: Pubkey,
    /// The vault this request is for.
    pub vault: Pubkey,
    /// Deposit mint amount held in the vault's deposit escrow.
    pub amount: u64,
    /// Epoch the request was made in.
    pub epoch: u64,
    pub bump: u8,
}

impl DepositRequest {
    pub fn new(args: NewDepositRequestArgs) -> Self {
        Self {
            authority: args.authority,
            vault: args.vault,
            amount: 0,
            epoch: args.epoch,
            bump: args.bump,
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
    pub fn add(&mut self, amount: u64, epoch: u64) -> Result<()> {
        validate!(
            self.epoch == epoch,
            HedgeVaultError::PendingRequestNotResolved
        )?;

        self.amount.safe_add_assign(amount)
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
