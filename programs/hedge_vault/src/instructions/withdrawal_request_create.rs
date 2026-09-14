use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::WithdrawalRequested,
    seeds::{CONFIG, SHARE_ESCROW, VAULT, WITHDRAWAL_REQUEST},
    validate, vault_seeds, withdrawal_request_seeds, Config, NewWithdrawalRequestArgs, Vault,
    WithdrawalRequest,
};

#[derive(Accounts)]
pub struct WithdrawalRequestCreate<'info> {
    #[account(mut)]
    pub withdrawer: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        init_if_needed,
        payer = withdrawer,
        space = WithdrawalRequest::DISCRIMINATOR.len() + WithdrawalRequest::INIT_SPACE,
        seeds = [WITHDRAWAL_REQUEST, vault.key().as_ref(), withdrawer.key().as_ref()],
        bump,
    )]
    pub withdrawal_request: Account<'info, WithdrawalRequest>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = share_mint,
        associated_token::authority = withdrawer,
        associated_token::token_program = share_token_program,
    )]
    pub withdrawer_share_token_account: InterfaceAccount<'info, TokenAccount>,
    /// Created upfront so the request can be resolved permissionlessly.
    #[account(
        init_if_needed,
        payer = withdrawer,
        associated_token::mint = deposit_mint,
        associated_token::authority = withdrawer,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub withdrawer_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [SHARE_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub share_escrow: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> WithdrawalRequestCreate<'info> {
    pub fn handler(ctx: Context<WithdrawalRequestCreate>, shares: u64) -> Result<()> {
        let WithdrawalRequestCreate {
            withdrawer,
            config,
            vault,
            withdrawal_request,
            deposit_mint,
            share_mint,
            withdrawer_share_token_account,
            share_escrow,
            share_token_program,
            ..
        } = ctx.accounts;

        validate!(shares > 0, HedgeVaultError::InvalidSharesAmount)?;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_withdrawable()?;

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.is_vault_withdrawable()?;
        vault.validate_deposit_mint(deposit_mint.key())?;
        vault.validate_share_mint(share_mint.key())?;

        let now = Clock::get()?.unix_timestamp;
        let epoch = Vault::epoch(now);
        let withdrawer_key = withdrawer.key();

        // conditionally initialize withdrawal request
        if withdrawal_request.authority == Pubkey::default() {
            withdrawal_request.set_inner(WithdrawalRequest::new(NewWithdrawalRequestArgs {
                authority: withdrawer_key,
                vault: vault_key,
                epoch,
                created_ts: now,
                bump: ctx.bumps.withdrawal_request,
            }));
        } else {
            let withdrawal_request_key = withdrawal_request.key();
            let withdrawal_request_bump = withdrawal_request.bump;
            let withdrawal_request_seeds =
                withdrawal_request_seeds!(vault_key, withdrawer_key, withdrawal_request_bump);

            WithdrawalRequest::validate_address(
                withdrawal_request_seeds,
                withdrawal_request_key,
            )?;
        }

        withdrawal_request.add(shares, epoch)?;
        vault.request_withdrawal(shares, withdrawer_share_token_account.amount)?;

        transfer_checked(
            CpiContext::new(
                share_token_program.to_account_info(),
                TransferChecked {
                    authority: withdrawer.to_account_info(),
                    from: withdrawer_share_token_account.to_account_info(),
                    mint: share_mint.to_account_info(),
                    to: share_escrow.to_account_info(),
                },
            ),
            shares,
            share_mint.decimals,
        )?;

        emit!(WithdrawalRequested {
            vault: vault_key,
            authority: withdrawer_key,
            shares,
            pending_shares: withdrawal_request.shares,
            epoch: withdrawal_request.epoch,
        });

        Ok(())
    }
}
