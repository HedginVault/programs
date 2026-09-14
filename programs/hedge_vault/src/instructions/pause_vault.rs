use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    events::VaultPaused,
    seeds::{CONFIG, VAULT},
    vault_seeds, Config, Vault,
};

#[derive(Accounts)]
pub struct PauseVault<'info> {
    pub guardian: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
}

impl<'info> PauseVault<'info> {
    pub fn handler(ctx: Context<PauseVault>) -> Result<()> {
        let PauseVault {
            guardian,
            config,
            vault,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_guardian(guardian.key())?;

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;

        vault.pause();

        emit!(VaultPaused {
            vault: vault_key,
            guardian: guardian.key(),
        });

        Ok(())
    }
}
