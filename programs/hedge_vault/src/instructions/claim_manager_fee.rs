use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::{mint_to, MintTo},
    token_interface::{Mint, TokenAccount},
};

use crate::{events::ManagerFeeClaimed, seeds::VAULT, vault_seeds, Vault};

#[derive(Accounts)]
pub struct ClaimManagerFee<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = share_mint,
        associated_token::authority = authority,
        associated_token::token_program = share_token_program,
    )]
    pub authority_share_token_account: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> ClaimManagerFee<'info> {
    pub fn handler(ctx: Context<ClaimManagerFee>) -> Result<()> {
        let ClaimManagerFee {
            authority,
            vault,
            share_mint,
            authority_share_token_account,
            share_token_program,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.validate_share_mint(share_mint.key())?;

        let shares = vault.claim_manager_fee()?;

        mint_to(
            CpiContext::new(
                share_token_program.to_account_info(),
                MintTo {
                    mint: share_mint.to_account_info(),
                    to: authority_share_token_account.to_account_info(),
                    authority: vault_acc_info,
                },
            )
            .with_signer(&[vault_seeds]),
            shares,
        )?;

        emit!(ManagerFeeClaimed {
            vault: vault_key,
            authority: authority.key(),
            shares,
        });

        Ok(())
    }
}
