use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{AssociatedToken, Create},
    token::Token,
    token_2022::{mint_to, transfer_checked, MintTo, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds, create_payout_account, deposit_request_seeds,
    events::DepositResolved,
    seeds::{CONFIG, DEPOSIT_ESCROW, DEPOSIT_REQUEST, VAULT},
    vault_seeds, Config, DepositRequest, Vault,
};

/// Permissionless, anyone can resolve a request once NAV is updated.
#[derive(Accounts)]
pub struct DepositRequestResolve<'info> {
    #[account(mut)]
    pub resolver: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    /// CHECK: Request authority, validated in [handler]. Receives rent of the closed request.
    #[account(mut)]
    pub depositor: UncheckedAccount<'info>,
    #[account(
        mut,
        close = depositor,
    )]
    pub deposit_request: Account<'info, DepositRequest>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,
    /// CHECK: The depositor's share account, created in [handler] when they closed it. Seeds spell
    /// out the associated token address so the constraint checks it and clients still derive it.
    #[account(
        mut,
        seeds = [depositor.key().as_ref(), share_token_program.key().as_ref(), share_mint.key().as_ref()],
        bump,
        seeds::program = associated_token_program,
    )]
    pub depositor_share_token_account: UncheckedAccount<'info>,
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
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> DepositRequestResolve<'info> {
    pub fn handler(ctx: Context<DepositRequestResolve>) -> Result<()> {
        let DepositRequestResolve {
            resolver,
            config,
            vault,
            depositor,
            deposit_request,
            deposit_mint,
            share_mint,
            depositor_share_token_account,
            vault_token_account,
            deposit_escrow,
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
        config.is_protocol_operational()?;

        let vault_acc_info = vault.to_account_info();

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.is_vault_operational()?;
        vault.validate_deposit_not_paused()?;
        vault.validate_deposit_mint(deposit_mint.key())?;
        vault.validate_share_mint(share_mint.key())?;

        let depositor_key = depositor.key();
        let deposit_request_key = deposit_request.key();
        let deposit_request_bump = deposit_request.bump;
        let deposit_request_seeds =
            deposit_request_seeds!(vault_key, depositor_key, deposit_request_bump);

        DepositRequest::validate_address(deposit_request_seeds, deposit_request_key)?;
        deposit_request.validate_authority(depositor_key)?;
        deposit_request.validate_vault(vault_key)?;
        deposit_request.is_resolvable(vault.nav_epoch)?;

        let amount = deposit_request.amount;
        let rent_escrow = deposit_request.rent_escrow;
        let shares = vault.resolve_deposit(amount)?;
        let nav_per_share = vault.nav_per_share;

        // CPIs borrow every passed account, the vault signs so its data must not stay borrowed
        drop(vault);

        create_payout_account(
            CpiContext::new(
                associated_token_program.to_account_info(),
                Create {
                    payer: resolver.to_account_info(),
                    associated_token: depositor_share_token_account.to_account_info(),
                    authority: depositor.to_account_info(),
                    mint: share_mint.to_account_info(),
                    system_program: system_program.to_account_info(),
                    token_program: share_token_program.to_account_info(),
                },
            ),
            &deposit_request.to_account_info(),
            rent_escrow,
        )?;

        transfer_checked(
            CpiContext::new(
                deposit_mint_token_program.to_account_info(),
                TransferChecked {
                    authority: vault_acc_info.clone(),
                    from: deposit_escrow.to_account_info(),
                    mint: deposit_mint.to_account_info(),
                    to: vault_token_account.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds]),
            amount,
            deposit_mint.decimals,
        )?;

        mint_to(
            CpiContext::new(
                share_token_program.to_account_info(),
                MintTo {
                    mint: share_mint.to_account_info(),
                    to: depositor_share_token_account.to_account_info(),
                    authority: vault_acc_info,
                },
            )
            .with_signer(&[vault_seeds]),
            shares,
        )?;

        emit!(DepositResolved {
            vault: vault_key,
            authority: depositor_key,
            amount,
            shares,
            nav_per_share,
        });

        Ok(())
    }
}
