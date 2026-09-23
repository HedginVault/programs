use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::PhoenixOrderPlaced,
    protocol::phoenix::{
        OrderPacket, PhoenixCpi, PhoenixGlobalConfig, PhoenixMarket, PhoenixOrderKind,
        PhoenixSelfTradeBehavior, PhoenixSide, HAWKEYE_PROGRAM_ID, PHOENIX_GLOBAL_CONFIGURATION,
        PHOENIX_LOG_AUTHORITY, PHOENIX_PROGRAM_ID,
    },
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

/// Immediate-or-cancel order, sized off-chain in Phoenix ticks and lots.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct PhoenixMarketOrderParams {
    pub side: PhoenixSide,
    /// Worst acceptable price, `None` takes any price.
    pub price_in_ticks: Option<u64>,
    pub num_base_lots: u64,
    pub num_quote_lots: Option<u64>,
    pub min_base_lots_to_fill: u64,
    pub min_quote_lots_to_fill: u64,
    pub self_trade_behavior: PhoenixSelfTradeBehavior,
    pub match_limit: Option<u64>,
    pub client_order_id: u128,
    pub last_valid_slot: Option<u64>,
    /// Closing a position is an exit path, so reduce-only orders stay open in reduce-only status.
    pub reduce_only: bool,
    pub cancel_existing: bool,
}

impl PhoenixMarketOrderParams {
    fn packet(&self) -> OrderPacket {
        OrderPacket::ImmediateOrCancel {
            side: self.side,
            price_in_ticks: self.price_in_ticks,
            num_base_lots: self.num_base_lots,
            num_quote_lots: self.num_quote_lots,
            min_base_lots_to_fill: self.min_base_lots_to_fill,
            min_quote_lots_to_fill: self.min_quote_lots_to_fill,
            self_trade_behavior: self.self_trade_behavior,
            match_limit: self.match_limit,
            client_order_id: self.client_order_id,
            last_valid_slot: self.last_valid_slot,
            order_flags: OrderPacket::order_flags(self.reduce_only),
            cancel_existing: self.cancel_existing,
        }
    }
}

#[derive(Accounts)]
pub struct PhoenixPlaceMarketOrder<'info> {
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
    /// CHECK: Hawkeye view program
    #[account(address = HAWKEYE_PROGRAM_ID)]
    pub hawkeye_program: UncheckedAccount<'info>,
}

impl<'info> PhoenixPlaceMarketOrder<'info> {
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, PhoenixPlaceMarketOrder<'info>>,
        params: PhoenixMarketOrderParams,
    ) -> Result<()> {
        let PhoenixPlaceMarketOrder {
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
            hawkeye_program,
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        if params.reduce_only {
            config.is_protocol_withdrawable()?;
            vault.is_vault_withdrawable()?;
        } else {
            config.is_protocol_operational()?;
            vault.is_vault_operational()?;
        }

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

        let phoenix = PhoenixCpi {
            phoenix_program: phoenix_program.to_account_info(),
            log_authority: log_authority.to_account_info(),
            global_config: global_config.to_account_info(),
            trader: vault_acc_info,
            trader_account: trader_account.to_account_info(),
            tail: ctx.remaining_accounts,
        };

        let response = phoenix.place_order(&market, &params.packet(), vault_seeds)?;

        validate!(
            response.base_lots_filled() >= params.min_base_lots_to_fill,
            HedgeVaultError::PhoenixOrderUnderfilled
        )?;
        validate!(
            !phoenix.is_liquidatable(hawkeye_program, &market.perp_asset_map)?,
            HedgeVaultError::PhoenixAccountLiquidatable
        )?;

        emit!(PhoenixOrderPlaced {
            vault: vault_key,
            strategy: strategy_key,
            orderbook: orderbook.key(),
            side: params.side,
            kind: PhoenixOrderKind::Market,
            price_in_ticks: params.price_in_ticks,
            num_base_lots: params.num_base_lots,
            reduce_only: params.reduce_only,
            base_lots_filled: response.base_lots_filled(),
            quote_lots_filled: response.quote_lots_filled(),
            base_lots_posted: response.num_base_lots_posted,
            order_sequence_number: response.order_sequence_number(),
            strategy_id: strategy.id,
        });

        Ok(())
    }
}
