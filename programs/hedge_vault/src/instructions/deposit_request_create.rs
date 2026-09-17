use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds, deposit_request_seeds,
    error::HedgeVaultError,
    events::DepositRequested,
    seeds::{CONFIG, DEPOSIT_ESCROW, DEPOSIT_REQUEST, VAULT},
    validate, validate_deposit_mint_extensions, vault_seeds, Config, DepositRequest,
    NewDepositRequestArgs, Vault,
};

#[derive(Accounts)]
pub struct DepositRequestCreate<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        init_if_needed,
        payer = depositor,
        space = DepositRequest::DISCRIMINATOR.len() + DepositRequest::INIT_SPACE,
        seeds = [DEPOSIT_REQUEST, vault.key().as_ref(), depositor.key().as_ref()],
        bump,
    )]
    pub deposit_request: Account<'info, DepositRequest>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = deposit_mint,
        associated_token::authority = depositor,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub depositor_token_account: InterfaceAccount<'info, TokenAccount>,
    /// Created upfront so the request can be resolved permissionlessly.
    #[account(
        init_if_needed,
        payer = depositor,
        associated_token::mint = share_mint,
        associated_token::authority = depositor,
        associated_token::token_program = share_token_program,
    )]
    pub depositor_share_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [DEPOSIT_ESCROW, vault.key().as_ref()],
        bump,
    )]
    pub deposit_escrow: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> DepositRequestCreate<'info> {
    pub fn handler(ctx: Context<DepositRequestCreate>, amount: u64) -> Result<()> {
        let DepositRequestCreate {
            depositor,
            config,
            vault,
            deposit_request,
            deposit_mint,
            share_mint,
            depositor_token_account,
            deposit_escrow,
            deposit_mint_token_program,
            ..
        } = ctx.accounts;

        validate!(amount > 0, HedgeVaultError::InvalidTransferAmount)?;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_operational()?;

        let vault_key = vault.key();
        let vault = &mut vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.is_vault_operational()?;
        vault.validate_deposit_not_paused()?;
        vault.validate_deposit_mint(deposit_mint.key())?;
        vault.validate_share_mint(share_mint.key())?;

        // the issuer can enable a transfer fee after the vault is created
        let clock = Clock::get()?;
        validate_deposit_mint_extensions(&deposit_mint.to_account_info(), clock.epoch)?;
        let now = clock.unix_timestamp;
        let epoch = Vault::epoch(now);
        let depositor_key = depositor.key();

        // conditionally initialize deposit request
        if deposit_request.authority == Pubkey::default() {
            deposit_request.set_inner(DepositRequest::new(NewDepositRequestArgs {
                authority: depositor_key,
                vault: vault_key,
                epoch,
                created_ts: now,
                bump: ctx.bumps.deposit_request,
            }));
        } else {
            let deposit_request_key = deposit_request.key();
            let deposit_request_bump = deposit_request.bump;
            let deposit_request_seeds =
                deposit_request_seeds!(vault_key, depositor_key, deposit_request_bump);

            DepositRequest::validate_address(deposit_request_seeds, deposit_request_key)?;
        }

        deposit_request.add(amount, epoch)?;
        vault.request_deposit(amount)?;

        transfer_checked(
            CpiContext::new(
                deposit_mint_token_program.to_account_info(),
                TransferChecked {
                    authority: depositor.to_account_info(),
                    from: depositor_token_account.to_account_info(),
                    mint: deposit_mint.to_account_info(),
                    to: deposit_escrow.to_account_info(),
                },
            ),
            amount,
            deposit_mint.decimals,
        )?;

        emit!(DepositRequested {
            vault: vault_key,
            authority: depositor_key,
            amount,
            pending_amount: deposit_request.amount,
            epoch: deposit_request.epoch,
        });

        Ok(())
    }
}
