use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::PhoenixOrdersCancelled,
    protocol::phoenix::{
        PhoenixCancelMode, PhoenixCpi, PhoenixGlobalConfig, PhoenixMarket,
        PHOENIX_GLOBAL_CONFIGURATION, PHOENIX_LOG_AUTHORITY, PHOENIX_PROGRAM_ID,
    },
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct PhoenixCancelOrders<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut)]
    pub strategy: Box<Account<'info, Strategy>>,
    /// CHECK: validated against strategy in [handler]
    #[account(mut)]
    pub trader_account: UncheckedAccount<'info>,
    /// CHECK: validated against the phoenix global configuration in [handler]
    #[account(mut)]
    pub perp_asset_map: UncheckedAccount<'info>,
    /// CHECK: market account, validated in phoenix program
    #[account(mut)]
    pub orderbook: UncheckedAccount<'info>,
    /// CHECK: spline PDA of the orderbook, validated in [handler]
    #[account(mut)]
    pub spline_collection: UncheckedAccount<'info>,
    /// CHECK: Phoenix global configuration
    #[account(mut, address = PHOENIX_GLOBAL_CONFIGURATION)]
    pub global_config: UncheckedAccount<'info>,
    /// CHECK: Phoenix log authority
    #[account(address = PHOENIX_LOG_AUTHORITY)]
    pub log_authority: UncheckedAccount<'info>,
    /// CHECK: Phoenix program
    #[account(address = PHOENIX_PROGRAM_ID)]
    pub phoenix_program: UncheckedAccount<'info>,
}

impl<'info> PhoenixCancelOrders<'info> {
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, PhoenixCancelOrders<'info>>,
        mode: PhoenixCancelMode,
    ) -> Result<()> {
        let PhoenixCancelOrders {
            authority,
            config,
            vault,
            strategy,
            trader_account,
            perp_asset_map,
            orderbook,
            spline_collection,
            global_config,
            log_authority,
            phoenix_program,
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        // cancelling reduces risk, it stays open while the protocol is reduce-only
        config.is_protocol_withdrawable()?;

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.is_vault_withdrawable()?;

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

        let market = PhoenixMarket {
            perp_asset_map: perp_asset_map.to_account_info(),
            orderbook: orderbook.to_account_info(),
            spline_collection: spline_collection.to_account_info(),
        };

        let phoenix_config = PhoenixGlobalConfig::load(global_config)?;
        market.validate(&phoenix_config)?;
        phoenix_config.validate_trader_tail(ctx.remaining_accounts)?;

        let now = Clock::get()?.unix_timestamp;
        strategy.record_action(now);
        drop(vault);

        PhoenixCpi {
            phoenix_program: phoenix_program.to_account_info(),
            log_authority: log_authority.to_account_info(),
            global_config: global_config.to_account_info(),
            trader: vault_acc_info,
            trader_account: trader_account.to_account_info(),
            tail: ctx.remaining_accounts,
        }
        .cancel_orders(&market, &mode, vault_seeds)?;

        emit!(PhoenixOrdersCancelled {
            vault: vault_key,
            strategy: strategy_key,
            orderbook: orderbook.key(),
            mode,
            strategy_id: strategy.id,
        });

        Ok(())
    }
}
