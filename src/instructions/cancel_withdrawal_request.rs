use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    events::WithdrawalCancelled,
    seeds::{SHARE_ESCROW, VAULT, WITHDRAWAL_REQUEST},
    vault_seeds, withdrawal_request_seeds, Vault, WithdrawalRequest,
};

#[derive(Accounts)]
pub struct CancelWithdrawalRequest<'info> {
    #[account(mut)]
    pub withdrawer: Signer<'info>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        mut,
        close = withdrawer,
    )]
    pub withdrawal_request: Account<'info, WithdrawalRequest>,
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = share_mint,
        associated_token::authority = withdrawer,
        associated_token::token_program = share_token_program,
    )]
    pub withdrawer_share_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [SHARE_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub share_escrow: InterfaceAccount<'info, TokenAccount>,
    pub share_token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

impl<'info> CancelWithdrawalRequest<'info> {
    pub fn handler(ctx: Context<CancelWithdrawalRequest>) -> Result<()> {
        let CancelWithdrawalRequest {
            withdrawer,
            vault,
            withdrawal_request,
            share_mint,
            withdrawer_share_token_account,
            share_escrow,
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
        vault.validate_share_mint(share_mint.key())?;

        let withdrawer_key = withdrawer.key();
        let withdrawal_request_key = withdrawal_request.key();
        let withdrawal_request_bump = withdrawal_request.bump;
        let withdrawal_request_seeds =
            withdrawal_request_seeds!(vault_key, withdrawer_key, withdrawal_request_bump);

        WithdrawalRequest::validate_address(withdrawal_request_seeds, withdrawal_request_key)?;
        withdrawal_request.validate_authority(withdrawer_key)?;
        withdrawal_request.validate_vault(vault_key)?;
        // once a NAV for the request is posted it must be resolved at that price
        withdrawal_request.is_cancellable(vault.nav_epoch)?;

        let shares = withdrawal_request.shares;
        vault.cancel_withdrawal(shares)?;

        transfer_checked(
            CpiContext::new(
                share_token_program.to_account_info(),
                TransferChecked {
                    authority: vault_acc_info,
                    from: share_escrow.to_account_info(),
                    mint: share_mint.to_account_info(),
                    to: withdrawer_share_token_account.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds]),
            shares,
            share_mint.decimals,
        )?;

        emit!(WithdrawalCancelled {
            vault: vault_key,
            authority: withdrawer_key,
            shares,
        });

        Ok(())
    }
}
