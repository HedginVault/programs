use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::NavUpdated,
    seeds::{CONFIG, VAULT},
    validate, vault_seeds, Config, UpdateNavArgs, Vault,
};

#[derive(Accounts)]
pub struct UpdateNav<'info> {
    pub nav_updater: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        associated_token::mint = deposit_mint,
        associated_token::authority = vault,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
}

impl<'info> UpdateNav<'info> {
    pub fn handler(ctx: Context<UpdateNav>, total_assets: u64) -> Result<()> {
        let UpdateNav {
            nav_updater,
            config,
            vault,
            deposit_mint,
            share_mint,
            vault_token_account,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_nav_updater(nav_updater.key())?;

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_authority = vault.authority.key();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_authority, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_deposit_mint(deposit_mint.key())?;
        vault.validate_share_mint(share_mint.key())?;

        validate!(
            total_assets >= vault_token_account.amount,
            HedgeVaultError::TotalAssetsBelowIdleBalance
        )?;

        let now = Clock::get()?.unix_timestamp;

        let nav_update = vault.update_nav(UpdateNavArgs {
            total_assets,
            share_supply: share_mint.supply,
            platform_performance_fee_bps: config.platform_performance_fee_bps,
            platform_management_fee_bps: config.platform_management_fee_bps,
            max_nav_deviation_bps: Some(config.max_nav_deviation_bps),
            now,
        })?;

        emit!(NavUpdated {
            vault: vault_key,
            epoch: vault.nav_epoch,
            total_assets: vault.total_assets,
            nav_per_share: vault.nav_per_share,
            high_water_mark: vault.high_water_mark,
            manager_fee_shares: nav_update.manager_fee_shares,
            platform_fee_shares: nav_update.platform_fee_shares,
            overridden: false,
        });

        Ok(())
    }
}
