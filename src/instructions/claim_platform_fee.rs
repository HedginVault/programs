use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::{mint_to, MintTo},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    config_seeds,
    events::PlatformFeeClaimed,
    seeds::{CONFIG, VAULT},
    vault_seeds, Config, Vault,
};

#[derive(Accounts)]
pub struct ClaimPlatformFee<'info> {
    #[account(mut)]
    pub treasury_authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = treasury_authority,
        associated_token::mint = share_mint,
        associated_token::authority = treasury_authority,
        associated_token::token_program = share_token_program,
    )]
    pub treasury_authority_share_token_account: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> ClaimPlatformFee<'info> {
    pub fn handler(ctx: Context<ClaimPlatformFee>) -> Result<()> {
        let ClaimPlatformFee {
            treasury_authority,
            config,
            vault,
            share_mint,
            treasury_authority_share_token_account,
            share_token_program,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_treasury_authority(treasury_authority.key())?;

        let vault_acc_info = vault.to_account_info();

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_authority = vault.authority.key();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_authority, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_share_mint(share_mint.key())?;

        let shares = vault.claim_platform_fee()?;

        mint_to(
            CpiContext::new(
                share_token_program.to_account_info(),
                MintTo {
                    mint: share_mint.to_account_info(),
                    to: treasury_authority_share_token_account.to_account_info(),
                    authority: vault_acc_info,
                },
            )
            .with_signer(&[vault_seeds]),
            shares,
        )?;

        emit!(PlatformFeeClaimed {
            vault: vault_key,
            authority: treasury_authority.key(),
            shares,
        });

        Ok(())
    }
}
