use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_interface::{Mint, TokenAccount},
};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::StrategyInitialized,
    protocol::phoenix::{
        trader_address, PhoenixCpi, PhoenixGlobalConfig, PhoenixTrader,
        PHOENIX_GLOBAL_CONFIGURATION, PHOENIX_LOG_AUTHORITY, PHOENIX_PROGRAM_ID, USDC_MINT,
    },
    seeds::{CONFIG, STRATEGY, VAULT},
    validate, vault_seeds, Config, NewStrategyArgs, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct PhoenixInitializeStrategy<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        init,
        payer = authority,
        space = Strategy::DISCRIMINATOR.len() + Strategy::INIT_SPACE + StrategyType::space(&StrategyType::PhoenixPerp { trader_account: trader_account.key() }),
        seeds = [STRATEGY, vault.key().as_ref(), trader_account.key().as_ref()],
        bump,
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    /// CHECK: vault's Phoenix trader PDA, re-derived in [handler], created by phoenix program
    #[account(mut)]
    pub trader_account: UncheckedAccount<'info>,
    /// validated against the phoenix global configuration in [handler]
    pub canonical_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = canonical_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_canonical_token_account: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: Phoenix global configuration
    #[account(address = PHOENIX_GLOBAL_CONFIGURATION)]
    pub global_config: UncheckedAccount<'info>,
    /// CHECK: Phoenix log authority
    #[account(address = PHOENIX_LOG_AUTHORITY)]
    pub log_authority: UncheckedAccount<'info>,
    /// CHECK: Phoenix program
    #[account(address = PHOENIX_PROGRAM_ID)]
    pub phoenix_program: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> PhoenixInitializeStrategy<'info> {
    pub fn handler(ctx: Context<PhoenixInitializeStrategy<'info>>) -> Result<()> {
        let PhoenixInitializeStrategy {
            authority,
            config,
            vault,
            strategy,
            trader_account,
            canonical_mint,
            global_config,
            log_authority,
            phoenix_program,
            system_program,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_operational()?;

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.is_vault_operational()?;

        // Phoenix collateral is USDC only, phoenix_deposit_funds draws from the idle deposit ATA
        validate!(
            vault.deposit_mint == USDC_MINT,
            HedgeVaultError::InvalidPhoenixDepositMint
        )?;

        // the strategy seed is the trader PDA, so a vault holds at most one Phoenix strategy
        validate!(
            trader_account.key() == trader_address(&vault_key),
            HedgeVaultError::InvalidPhoenixTrader
        )?;

        let phoenix_config = PhoenixGlobalConfig::load(global_config)?;
        PhoenixGlobalConfig::validate_account(phoenix_config.canonical_mint, canonical_mint.key())?;

        let now = Clock::get()?.unix_timestamp;

        strategy.set_inner(Strategy::new(NewStrategyArgs {
            vault: vault_key,
            id: vault.next_strategy_id,
            bump: ctx.bumps.strategy,
            created_ts: now,
            strategy_type: StrategyType::PhoenixPerp {
                trader_account: trader_account.key(),
            },
        }));

        vault.increment_strategy_id()?;
        vault.increment_open_strategies()?;
        drop(vault);

        // the Phoenix builder API registers the trader itself when onboarding runs first
        if trader_account.data_is_empty() {
            PhoenixCpi {
                phoenix_program: phoenix_program.to_account_info(),
                log_authority: log_authority.to_account_info(),
                global_config: global_config.to_account_info(),
                trader: vault_acc_info,
                trader_account: trader_account.to_account_info(),
                tail: &[],
            }
            .register_trader(
                &authority.to_account_info(),
                &system_program.to_account_info(),
            )?;
        } else {
            PhoenixTrader::load(trader_account)?.validate_authority(vault_key)?;
        }

        emit!(StrategyInitialized {
            vault: vault_key,
            strategy: strategy.key(),
            id: strategy.id,
            strategy_type: strategy.strategy_type,
            created_ts: now,
        });

        Ok(())
    }
}
