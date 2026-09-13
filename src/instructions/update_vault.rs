use anchor_lang::prelude::*;

use crate::{
    error::HedgeVaultError, seeds::VAULT, validate, vault_seeds, Vault, VaultStatus, MAX_BPS,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateVaultArgs {
    pub description: Option<[u8; 64]>,
    pub performance_fee_bps: Option<u16>,
    pub management_fee_bps: Option<u16>,
    pub deposit_cap: Option<u64>,
    pub status: Option<VaultStatus>,
}

#[derive(Accounts)]
pub struct UpdateVault<'info> {
    pub authority: Signer<'info>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
}

impl<'info> UpdateVault<'info> {
    pub fn handler(ctx: Context<UpdateVault>, args: UpdateVaultArgs) -> Result<()> {
        let UpdateVault {
            authority, vault, ..
        } = ctx.accounts;

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_authority = vault.authority.key();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_authority, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        if let Some(description) = args.description {
            vault.description = description;
        }

        if let Some(performance_fee_bps) = args.performance_fee_bps {
            validate!(
                performance_fee_bps <= MAX_BPS,
                HedgeVaultError::InvalidBasisPoints
            )?;

            vault.performance_fee_bps = performance_fee_bps;
        }

        if let Some(management_fee_bps) = args.management_fee_bps {
            validate!(
                management_fee_bps <= MAX_BPS,
                HedgeVaultError::InvalidBasisPoints
            )?;

            vault.management_fee_bps = management_fee_bps;
        }

        if let Some(deposit_cap) = args.deposit_cap {
            vault.deposit_cap = deposit_cap;
        }

        if let Some(status) = args.status {
            vault.status = status;
        }

        Ok(())
    }
}
