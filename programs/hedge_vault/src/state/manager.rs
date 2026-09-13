use anchor_lang::prelude::*;

use crate::{error::HedgeVaultError, validate, validate_pda};

pub struct NewManagerArgs {
    pub authority: Pubkey,
    pub bump: u8,
}

/// Whitelist entry allowing `authority` to create vaults. Created and closed by the config admin.
#[account]
#[derive(InitSpace)]
pub struct Manager {
    /// Authority allowed to create vaults.
    pub authority: Pubkey,
    pub bump: u8,
}

impl Manager {
    pub fn new(args: NewManagerArgs) -> Self {
        Self {
            authority: args.authority,
            bump: args.bump,
        }
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidManager.into())
    }

    pub fn validate_authority(&self, authority: Pubkey) -> Result<()> {
        validate!(
            authority == self.authority,
            HedgeVaultError::InvalidManagerAuthority
        )?;

        Ok(())
    }
}
