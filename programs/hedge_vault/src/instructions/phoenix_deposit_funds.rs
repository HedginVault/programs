use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_interface::{Mint, TokenAccount},
};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::PhoenixFundsDeposited,
    protocol::phoenix::{
        EmberCpi, PhoenixCpi, PhoenixGlobalConfig, PhoenixTrader, EMBER_PROGRAM_ID, EMBER_STATE,
        EMBER_VAULT, PHOENIX_GLOBAL_CONFIGURATION, PHOENIX_LOG_AUTHORITY, PHOENIX_PROGRAM_ID,
        USDC_MINT,
    },
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct PhoenixDepositFunds<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut)]
    pub strategy: Box<Account<'info, Strategy>>,
    #[account(address = USDC_MINT)]
    pub usdc_mint: InterfaceAccount<'info, Mint>,
    /// validated against the phoenix global configuration in [handler]
    #[account(mut)]
    pub canonical_mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_usdc_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = canonical_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_canonical_token_account: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: validated against strategy in [handler]
    #[account(mut)]
    pub trader_account: UncheckedAccount<'info>,
    /// CHECK: Ember state
    #[account(address = EMBER_STATE)]
    pub ember_state: UncheckedAccount<'info>,
    /// CHECK: Ember USDC custody
    #[account(mut, address = EMBER_VAULT)]
    pub ember_vault: UncheckedAccount<'info>,
    /// CHECK: validated against the phoenix global configuration in [handler]
    #[account(mut)]
    pub global_vault: UncheckedAccount<'info>,
    /// CHECK: Phoenix global configuration
    #[account(mut, address = PHOENIX_GLOBAL_CONFIGURATION)]
    pub global_config: UncheckedAccount<'info>,
    /// CHECK: Phoenix log authority
    #[account(address = PHOENIX_LOG_AUTHORITY)]
    pub log_authority: UncheckedAccount<'info>,
    /// CHECK: Phoenix program
    #[account(address = PHOENIX_PROGRAM_ID)]
    pub phoenix_program: UncheckedAccount<'info>,
    /// CHECK: Ember program
    #[account(address = EMBER_PROGRAM_ID)]
    pub ember_program: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

impl<'info> PhoenixDepositFunds<'info> {
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, PhoenixDepositFunds<'info>>,
        amount: u64,
    ) -> Result<()> {
        let PhoenixDepositFunds {
            authority,
            config,
            vault,
            strategy,
            usdc_mint,
            canonical_mint,
            vault_usdc_token_account,
            vault_canonical_token_account,
            trader_account,
            ember_state,
            ember_vault,
            global_vault,
            global_config,
            log_authority,
            phoenix_program,
            ember_program,
            token_program,
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
        vault.is_vault_operational()?;

        let trader_key = trader_account.key();

        let strategy_key = strategy.key();
        let strategy: &mut Strategy = strategy.as_mut();
        let strategy_bump = strategy.bump;
        let strategy_seeds = strategy_seeds!(vault_key, trader_key, strategy_bump);

        Strategy::validate_address(strategy_seeds, strategy_key)?;

        let StrategyType::PhoenixPerp {
            trader_account: strategy_trader_account,
        } = strategy.strategy_type
        else {
            return err!(HedgeVaultError::InvalidStrategyType);
        };

        validate!(
            strategy_trader_account == trader_key,
            HedgeVaultError::InvalidPhoenixTrader
        )?;

        validate!(amount > 0, HedgeVaultError::InvalidTransferAmount)?;

        let trader = PhoenixTrader::load(trader_account)?;
        trader.validate_authority(vault_key)?;
        trader.is_ready()?;

        let phoenix_config = PhoenixGlobalConfig::load(global_config)?;
        PhoenixGlobalConfig::validate_account(phoenix_config.canonical_mint, canonical_mint.key())?;
        PhoenixGlobalConfig::validate_account(phoenix_config.global_vault, global_vault.key())?;
        phoenix_config.validate_trader_tail(ctx.remaining_accounts)?;

        let now = Clock::get()?.unix_timestamp;
        strategy.record_action(now);
        drop(vault);

        let canonical_balance_before = vault_canonical_token_account.amount;

        EmberCpi {
            ember_program: ember_program.to_account_info(),
            trader: vault_acc_info.clone(),
            ember_state: ember_state.to_account_info(),
            usdc_mint: usdc_mint.to_account_info(),
            canonical_mint: canonical_mint.to_account_info(),
            trader_usdc_account: vault_usdc_token_account.to_account_info(),
            trader_canonical_account: vault_canonical_token_account.to_account_info(),
            ember_vault: ember_vault.to_account_info(),
            token_program: token_program.to_account_info(),
        }
        .deposit(amount, vault_seeds)?;

        PhoenixCpi {
            phoenix_program: phoenix_program.to_account_info(),
            log_authority: log_authority.to_account_info(),
            global_config: global_config.to_account_info(),
            trader: vault_acc_info,
            trader_account: trader_account.to_account_info(),
            tail: ctx.remaining_accounts,
        }
        .deposit_funds(
            &vault_canonical_token_account.to_account_info(),
            &global_vault.to_account_info(),
            &token_program.to_account_info(),
            amount,
            vault_seeds,
        )?;

        // Ember minted exactly `amount` and Phoenix took exactly `amount`
        vault_canonical_token_account.reload()?;
        validate!(
            vault_canonical_token_account.amount == canonical_balance_before,
            HedgeVaultError::PhoenixAmountMismatch
        )?;

        emit!(PhoenixFundsDeposited {
            vault: vault_key,
            strategy: strategy_key,
            trader_account: trader_key,
            amount,
            strategy_id: strategy.id,
        });

        Ok(())
    }
}
