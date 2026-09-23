use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_interface::{Mint, TokenAccount},
};

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::PhoenixCanonicalUnwrapped,
    protocol::phoenix::{
        EmberCpi, PhoenixGlobalConfig, EMBER_PROGRAM_ID, EMBER_STATE, EMBER_VAULT,
        PHOENIX_GLOBAL_CONFIGURATION, USDC_MINT,
    },
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct PhoenixEmberWithdraw<'info> {
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
    /// CHECK: Ember state
    #[account(address = EMBER_STATE)]
    pub ember_state: UncheckedAccount<'info>,
    /// CHECK: Ember USDC custody
    #[account(mut, address = EMBER_VAULT)]
    pub ember_vault: UncheckedAccount<'info>,
    /// CHECK: Phoenix global configuration
    #[account(address = PHOENIX_GLOBAL_CONFIGURATION)]
    pub global_config: UncheckedAccount<'info>,
    /// CHECK: Ember program
    #[account(address = EMBER_PROGRAM_ID)]
    pub ember_program: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

impl<'info> PhoenixEmberWithdraw<'info> {
    pub fn handler(ctx: Context<PhoenixEmberWithdraw<'info>>) -> Result<()> {
        let PhoenixEmberWithdraw {
            authority,
            config,
            vault,
            strategy,
            usdc_mint,
            canonical_mint,
            vault_usdc_token_account,
            vault_canonical_token_account,
            ember_state,
            ember_vault,
            global_config,
            ember_program,
            token_program,
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        // unwrapping collateral is an exit path, it stays open while the protocol is reduce-only
        config.is_protocol_withdrawable()?;

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.is_vault_withdrawable()?;

        let strategy_key = strategy.key();
        let strategy: &mut Strategy = strategy.as_mut();

        let StrategyType::PhoenixPerp { trader_account } = strategy.strategy_type else {
            return err!(HedgeVaultError::InvalidStrategyType);
        };

        Strategy::validate_address(
            strategy_seeds!(vault_key, trader_account, strategy.bump),
            strategy_key,
        )?;

        let phoenix_config = PhoenixGlobalConfig::load(global_config)?;
        PhoenixGlobalConfig::validate_account(phoenix_config.canonical_mint, canonical_mint.key())?;

        let amount = vault_canonical_token_account.amount;
        validate!(amount > 0, HedgeVaultError::InvalidTransferAmount)?;

        let now = Clock::get()?.unix_timestamp;
        strategy.record_action(now);
        drop(vault);

        EmberCpi {
            ember_program: ember_program.to_account_info(),
            trader: vault_acc_info,
            ember_state: ember_state.to_account_info(),
            usdc_mint: usdc_mint.to_account_info(),
            canonical_mint: canonical_mint.to_account_info(),
            trader_usdc_account: vault_usdc_token_account.to_account_info(),
            trader_canonical_account: vault_canonical_token_account.to_account_info(),
            ember_vault: ember_vault.to_account_info(),
            token_program: token_program.to_account_info(),
        }
        .withdraw(None, vault_seeds)?;

        emit!(PhoenixCanonicalUnwrapped {
            vault: vault_key,
            strategy: strategy_key,
            amount,
            strategy_id: strategy.id,
        });

        Ok(())
    }
}
