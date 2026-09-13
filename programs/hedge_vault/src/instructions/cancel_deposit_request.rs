use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    deposit_request_seeds,
    events::DepositCancelled,
    seeds::{DEPOSIT_ESCROW, DEPOSIT_REQUEST, VAULT},
    vault_seeds, DepositRequest, Vault,
};

#[derive(Accounts)]
pub struct CancelDepositRequest<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        mut,
        close = depositor,
    )]
    pub deposit_request: Account<'info, DepositRequest>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = deposit_mint,
        associated_token::authority = depositor,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub depositor_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [DEPOSIT_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub deposit_escrow: InterfaceAccount<'info, TokenAccount>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> CancelDepositRequest<'info> {
    pub fn handler(ctx: Context<CancelDepositRequest>) -> Result<()> {
        let CancelDepositRequest {
            depositor,
            vault,
            deposit_request,
            deposit_mint,
            depositor_token_account,
            deposit_escrow,
            deposit_mint_token_program,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_deposit_mint(deposit_mint.key())?;

        let depositor_key = depositor.key();
        let deposit_request_key = deposit_request.key();
        let deposit_request_bump = deposit_request.bump;
        let deposit_request_seeds =
            deposit_request_seeds!(vault_key, depositor_key, deposit_request_bump);

        DepositRequest::validate_address(deposit_request_seeds, deposit_request_key)?;
        deposit_request.validate_authority(depositor_key)?;
        deposit_request.validate_vault(vault_key)?;
        // once a NAV for the request is posted it must be resolved, unless NAV is zero
        deposit_request.is_cancellable(vault.nav_epoch, vault.nav_per_share)?;

        let amount = deposit_request.amount;
        vault.cancel_deposit(amount)?;

        // CPIs borrow every passed account, the vault signs so its data must not stay borrowed
        drop(vault);

        transfer_checked(
            CpiContext::new(
                deposit_mint_token_program.to_account_info(),
                TransferChecked {
                    authority: vault_acc_info,
                    from: deposit_escrow.to_account_info(),
                    mint: deposit_mint.to_account_info(),
                    to: depositor_token_account.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds]),
            amount,
            deposit_mint.decimals,
        )?;

        emit!(DepositCancelled {
            vault: vault_key,
            authority: depositor_key,
            amount,
        });

        Ok(())
    }
}
