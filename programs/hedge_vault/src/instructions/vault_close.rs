use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_2022::{close_account, CloseAccount},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    error::HedgeVaultError,
    events::VaultClosed,
    seeds::{DEPOSIT_ESCROW, SHARE_ESCROW, VAULT},
    validate, vault_seeds, Vault,
};

#[derive(Accounts)]
pub struct VaultClose<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        mut,
        close = authority,
    )]
    pub vault: AccountLoader<'info, Vault>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = deposit_mint,
        associated_token::authority = vault,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [DEPOSIT_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub deposit_escrow: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [SHARE_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub share_escrow: InterfaceAccount<'info, TokenAccount>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub share_token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

impl<'info> VaultClose<'info> {
    pub fn handler(ctx: Context<VaultClose>) -> Result<()> {
        let VaultClose {
            authority,
            vault,
            deposit_mint,
            share_mint,
            vault_token_account,
            deposit_escrow,
            share_escrow,
            deposit_mint_token_program,
            share_token_program,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.validate_deposit_mint(deposit_mint.key())?;
        vault.validate_share_mint(share_mint.key())?;

        validate!(
            share_mint.supply == 0,
            HedgeVaultError::VaultHasOutstandingShares
        )?;
        validate!(
            vault.pending_deposits == 0 && vault.pending_withdrawal_shares == 0,
            HedgeVaultError::VaultHasPendingRequests
        )?;
        validate!(
            vault.unclaimed_manager_fee_shares == 0 && vault.unclaimed_platform_fee_shares == 0,
            HedgeVaultError::VaultHasUnclaimedFees
        )?;
        // strategies hold positions owned by the vault PDA, closing first would strand them
        validate!(
            vault.open_strategy_count == 0,
            HedgeVaultError::VaultHasOpenStrategies
        )?;
        validate!(vault.total_assets == 0, HedgeVaultError::VaultHasAssets)?;

        drop(vault);

        // token accounts must be empty for close to succeed, remaining balances are the manager's to move out first
        for (token_program, token_account) in [
            (
                deposit_mint_token_program.to_account_info(),
                vault_token_account.to_account_info(),
            ),
            (
                deposit_mint_token_program.to_account_info(),
                deposit_escrow.to_account_info(),
            ),
            (
                share_token_program.to_account_info(),
                share_escrow.to_account_info(),
            ),
        ] {
            close_account(
                CpiContext::new(
                    token_program,
                    CloseAccount {
                        account: token_account,
                        authority: vault_acc_info.clone(),
                        destination: authority.to_account_info(),
                    },
                )
                .with_signer(&[vault_seeds]),
            )?;
        }

        emit!(VaultClosed { vault: vault_key });

        Ok(())
    }
}
