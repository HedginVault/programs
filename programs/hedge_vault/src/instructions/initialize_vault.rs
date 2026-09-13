use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::VaultInitialized,
    manager_seeds,
    seeds::{CONFIG, DEPOSIT_ESCROW, MANAGER, SHARE_ESCROW, SHARE_MINT, VAULT},
    validate, validate_deposit_mint_extensions, Config, Manager, NewVaultArgs, Vault, MAX_BPS,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitializeVaultArgs {
    pub name: [u8; 32],
    pub description: [u8; 64],
    pub performance_fee_bps: u16,
    pub management_fee_bps: u16,
    pub deposit_cap: u64,
    pub min_deposit: u64,
    pub min_withdrawal_shares: u64,
}

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub config: AccountLoader<'info, Config>,
    pub manager: Account<'info, Manager>,
    #[account(
        init,
        payer = authority,
        space = Vault::DISCRIMINATOR.len() + Vault::INIT_SPACE,
        seeds = [VAULT, config.load()?.next_vault_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub vault: AccountLoader<'info, Vault>,
    pub deposit_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        payer = authority,
        seeds = [SHARE_MINT, vault.key().as_ref()],
        bump,
        mint::decimals = deposit_mint.decimals,
        mint::authority = vault,
        mint::token_program = share_token_program,
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = deposit_mint,
        associated_token::authority = vault,
        associated_token::token_program = deposit_mint_token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    /// Holds pending deposits until resolved.
    #[account(
        init,
        payer = authority,
        seeds = [DEPOSIT_ESCROW, vault.key().as_ref()],
        bump,
        token::mint = deposit_mint,
        token::authority = vault,
        token::token_program = deposit_mint_token_program,
    )]
    pub deposit_escrow: InterfaceAccount<'info, TokenAccount>,
    /// Holds pending withdrawal shares until resolved.
    #[account(
        init,
        payer = authority,
        seeds = [SHARE_ESCROW, vault.key().as_ref()],
        bump,
        token::mint = share_mint,
        token::authority = vault,
        token::token_program = share_token_program,
    )]
    pub share_escrow: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub deposit_mint_token_program: Interface<'info, TokenInterface>,
    pub share_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> InitializeVault<'info> {
    pub fn handler(ctx: Context<InitializeVault>, args: InitializeVaultArgs) -> Result<()> {
        let InitializeVault {
            authority,
            config,
            manager,
            vault,
            deposit_mint,
            share_mint,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = &mut config.load_mut()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_operational()?;

        // vault creation is gated by the manager whitelist
        let authority_key = authority.key();
        let manager_key = manager.key();
        let manager_bump = manager.bump;
        let manager_seeds = manager_seeds!(authority_key, manager_bump);

        Manager::validate_address(manager_seeds, manager_key)?;
        manager.validate_authority(authority_key)?;

        validate!(
            args.performance_fee_bps <= MAX_BPS,
            HedgeVaultError::InvalidBasisPoints
        )?;
        validate!(
            args.management_fee_bps <= MAX_BPS,
            HedgeVaultError::InvalidBasisPoints
        )?;

        let clock = Clock::get()?;
        validate_deposit_mint_extensions(&deposit_mint.to_account_info(), clock.epoch)?;

        let vault_key = vault.key();
        let mut vault = vault.load_init()?;

        *vault = Vault::new(NewVaultArgs {
            id: config.next_vault_id,
            authority: authority_key,
            name: args.name,
            description: args.description,
            deposit_mint: deposit_mint.key(),
            share_mint: share_mint.key(),
            deposit_cap: args.deposit_cap,
            min_deposit: args.min_deposit,
            min_withdrawal_shares: args.min_withdrawal_shares,
            performance_fee_bps: args.performance_fee_bps,
            management_fee_bps: args.management_fee_bps,
            current_ts: clock.unix_timestamp,
            bump: ctx.bumps.vault,
        });

        config.increment_vault_id()?;

        emit!(VaultInitialized {
            vault: vault_key,
            id: vault.id,
            authority: vault.authority,
            deposit_mint: vault.deposit_mint,
            share_mint: vault.share_mint,
        });

        Ok(())
    }
}
