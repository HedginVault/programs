use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{AssociatedToken, Create},
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds, create_payout_account, deposit_request_seeds,
    events::DepositRejected,
    seeds::{CONFIG, DEPOSIT_ESCROW, DEPOSIT_REQUEST, VAULT},
    vault_seeds, Config, DepositRequest, Vault,
};

/// Admin refunds a pending deposit, e.g. for compliance. Allowed until the request is resolved
/// and not gated by protocol status, so it also works while paused.
#[derive(Accounts)]
pub struct DepositRequestReject<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    /// CHECK: Request authority, validated in [handler]. Receives the refund and the rent of the closed request.
    #[account(mut)]
    pub depositor: UncheckedAccount<'info>,
    #[account(
        mut,
        close = depositor,
    )]
    pub deposit_request: Account<'info, DepositRequest>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    /// CHECK: The depositor's refund account, created in [handler] when they closed it. Seeds spell
    /// out the associated token address so the constraint checks it and clients still derive it.
    #[account(
        mut,
        seeds = [depositor.key().as_ref(), deposit_mint_token_program.key().as_ref(), deposit_mint.key().as_ref()],
        bump,
        seeds::program = associated_token_program,
    )]
    pub depositor_token_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [DEPOSIT_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub deposit_escrow: InterfaceAccount<'info, TokenAccount>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> DepositRequestReject<'info> {
    pub fn handler(ctx: Context<DepositRequestReject>) -> Result<()> {
        let DepositRequestReject {
            admin,
            config,
            vault,
            depositor,
            deposit_request,
            deposit_mint,
            depositor_token_account,
            deposit_escrow,
            deposit_mint_token_program,
            associated_token_program,
            system_program,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_admin(admin.key())?;

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

        let amount = deposit_request.amount;
        let rent_escrow = deposit_request.rent_escrow;
        vault.cancel_deposit(amount)?;

        // CPIs borrow every passed account, the vault signs so its data must not stay borrowed
        drop(vault);

        create_payout_account(
            CpiContext::new(
                associated_token_program.to_account_info(),
                Create {
                    payer: admin.to_account_info(),
                    associated_token: depositor_token_account.to_account_info(),
                    authority: depositor.to_account_info(),
                    mint: deposit_mint.to_account_info(),
                    system_program: system_program.to_account_info(),
                    token_program: deposit_mint_token_program.to_account_info(),
                },
            ),
            &deposit_request.to_account_info(),
            rent_escrow,
        )?;

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

        emit!(DepositRejected {
            vault: vault_key,
            authority: depositor_key,
            amount,
        });

        Ok(())
    }
}
