use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{AssociatedToken, Create},
    token::Token,
    token_2022::{burn, transfer_checked, Burn, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds, create_payout_account,
    error::HedgeVaultError,
    events::WithdrawalResolved,
    seeds::{CONFIG, SHARE_ESCROW, VAULT, WITHDRAWAL_REQUEST},
    validate, vault_seeds, withdrawal_request_seeds, Config, Vault, WithdrawalRequest,
};

/// Permissionless, anyone can resolve a request once NAV is updated.
#[derive(Accounts)]
pub struct WithdrawalRequestResolve<'info> {
    #[account(mut)]
    pub resolver: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    /// CHECK: Request authority, validated in [handler]. Receives rent of the closed request.
    #[account(mut)]
    pub withdrawer: UncheckedAccount<'info>,
    #[account(
        mut,
        close = withdrawer,
    )]
    pub withdrawal_request: Account<'info, WithdrawalRequest>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,
    /// CHECK: The withdrawer's payout account, created in [handler] when they closed it. Seeds
    /// spell out the associated token address so the constraint checks it and clients still derive it.
    #[account(
        mut,
        seeds = [withdrawer.key().as_ref(), deposit_mint_token_program.key().as_ref(), deposit_mint.key().as_ref()],
        bump,
        seeds::program = associated_token_program,
    )]
    pub withdrawer_token_account: UncheckedAccount<'info>,
    #[account(
        mut,
        associated_token::mint = deposit_mint,
        associated_token::authority = vault,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [SHARE_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub share_escrow: InterfaceAccount<'info, TokenAccount>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> WithdrawalRequestResolve<'info> {
    pub fn handler(ctx: Context<WithdrawalRequestResolve>) -> Result<()> {
        let WithdrawalRequestResolve {
            resolver,
            config,
            vault,
            withdrawer,
            withdrawal_request,
            deposit_mint,
            share_mint,
            withdrawer_token_account,
            vault_token_account,
            share_escrow,
            deposit_mint_token_program,
            share_token_program,
            associated_token_program,
            system_program,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_withdrawable()?;

        let vault_acc_info = vault.to_account_info();

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.is_vault_withdrawable()?;
        vault.validate_withdrawal_not_paused()?;
        vault.validate_deposit_mint(deposit_mint.key())?;
        vault.validate_share_mint(share_mint.key())?;

        let withdrawer_key = withdrawer.key();
        let withdrawal_request_key = withdrawal_request.key();
        let withdrawal_request_bump = withdrawal_request.bump;
        let withdrawal_request_seeds =
            withdrawal_request_seeds!(vault_key, withdrawer_key, withdrawal_request_bump);

        WithdrawalRequest::validate_address(withdrawal_request_seeds, withdrawal_request_key)?;
        withdrawal_request.validate_authority(withdrawer_key)?;
        withdrawal_request.validate_vault(vault_key)?;
        withdrawal_request.is_resolvable(vault.nav_epoch)?;

        let shares = withdrawal_request.shares;
        let rent_escrow = withdrawal_request.rent_escrow;
        let amount = vault.resolve_withdrawal(shares, config.max_epoch_outflow_bps)?;

        // manager is expected to keep enough idle deposit mint in the vault to cover pending withdrawals
        validate!(
            vault_token_account.amount >= amount,
            HedgeVaultError::InsufficientFunds
        )?;

        let nav_per_share = vault.nav_per_share;

        // CPIs borrow every passed account, the vault signs so its data must not stay borrowed
        drop(vault);

        burn(
            CpiContext::new(
                share_token_program.to_account_info(),
                Burn {
                    mint: share_mint.to_account_info(),
                    from: share_escrow.to_account_info(),
                    authority: vault_acc_info.clone(),
                },
            )
            .with_signer(&[vault_seeds]),
            shares,
        )?;

        if amount > 0 {
            create_payout_account(
                CpiContext::new(
                    associated_token_program.to_account_info(),
                    Create {
                        payer: resolver.to_account_info(),
                        associated_token: withdrawer_token_account.to_account_info(),
                        authority: withdrawer.to_account_info(),
                        mint: deposit_mint.to_account_info(),
                        system_program: system_program.to_account_info(),
                        token_program: deposit_mint_token_program.to_account_info(),
                    },
                ),
                &withdrawal_request.to_account_info(),
                rent_escrow,
            )?;

            transfer_checked(
                CpiContext::new(
                    deposit_mint_token_program.to_account_info(),
                    TransferChecked {
                        authority: vault_acc_info,
                        from: vault_token_account.to_account_info(),
                        mint: deposit_mint.to_account_info(),
                        to: withdrawer_token_account.to_account_info(),
                    },
                )
                .with_signer(&[vault_seeds]),
                amount,
                deposit_mint.decimals,
            )?;
        }

        emit!(WithdrawalResolved {
            vault: vault_key,
            authority: withdrawer_key,
            shares,
            amount,
            nav_per_share,
        });

        Ok(())
    }
}
