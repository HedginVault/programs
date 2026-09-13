use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::JupiterSwapped,
    jupiter,
    protocol::jupiter::{JupiterSwapCpi, JUPITER_AGGREGATOR_EVENT_AUTHORITY},
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct JupiterSwap<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut)]
    pub strategy: Box<Account<'info, Strategy>>,
    pub source_mint: InterfaceAccount<'info, Mint>,
    pub destination_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub vault_source_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = destination_mint,
        associated_token::authority = vault,
        associated_token::token_program = destination_token_program,
    )]
    pub vault_destination_token_account: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub destination_token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Jupiter event authority
    #[account(address = JUPITER_AGGREGATOR_EVENT_AUTHORITY)]
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: Jupiter v6 aggregator
    #[account(address = jupiter::ID)]
    pub jupiter_program: UncheckedAccount<'info>,
}

impl<'info> JupiterSwap<'info> {
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, JupiterSwap<'info>>,
        swap_data: Vec<u8>,
        amount: u64,
        slippage_bps: u16,
    ) -> Result<()> {
        let JupiterSwap {
            authority,
            config,
            vault,
            strategy,
            source_mint,
            destination_mint,
            vault_source_token_account,
            vault_destination_token_account,
            destination_token_program,
            event_authority,
            jupiter_program,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_operational()?;

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        // one side of the swap is the deposit mint, the other is the strategy target mint
        let target_mint = if source_mint.key() == vault.deposit_mint {
            destination_mint.key()
        } else {
            vault.validate_deposit_mint(destination_mint.key())?;
            source_mint.key()
        };

        let strategy_key = strategy.key();
        let strategy: &mut Strategy = strategy.as_mut();
        let strategy_bump = strategy.bump;
        let strategy_seeds = strategy_seeds!(vault_key, target_mint, strategy_bump);

        Strategy::validate_address(strategy_seeds, strategy_key)?;

        let StrategyType::JupiterSwap {
            target_mint: strategy_target_mint,
        } = strategy.strategy_type
        else {
            return err!(HedgeVaultError::InvalidStrategyType);
        };

        validate!(
            strategy_target_mint == target_mint,
            HedgeVaultError::InvalidTargetMint
        )?;

        let now = Clock::get()?.unix_timestamp;
        strategy.record_action(now);
        drop(vault);

        JupiterSwapCpi::check_amount_and_slippage(&swap_data, amount, slippage_bps)?;

        let mut jupiter_swap = JupiterSwapCpi {
            event_authority: event_authority.to_account_info(),
            source_mint: source_mint.to_account_info(),
            source_token_account: vault_source_token_account.to_account_info(),
            jupiter_program: jupiter_program.to_account_info(),
            destination_mint: destination_mint.to_account_info(),
            destination_token_account: vault_destination_token_account.to_account_info(),
            token_account_authority: vault_acc_info,
            token_program: destination_token_program.to_account_info(),
        };

        jupiter_swap.swap(&swap_data, ctx.remaining_accounts, vault_seeds)?;

        emit!(JupiterSwapped {
            vault: vault_key,
            strategy: strategy_key,
            source_mint: source_mint.key(),
            destination_mint: destination_mint.key(),
            amount,
        });

        Ok(())
    }
}
