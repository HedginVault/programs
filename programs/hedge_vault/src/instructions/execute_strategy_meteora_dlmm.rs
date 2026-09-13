use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds,
    dlmm::{
        self,
        cpi::{accounts::AddLiquidityByStrategy2, add_liquidity_by_strategy2},
        types::{LiquidityParameterByStrategy, RemainingAccountsInfo},
    },
    error::HedgeVaultError,
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ExecuteStrategyMeteoraDlmmParams {
    pub liquidity_parameter: LiquidityParameterByStrategy,
    pub remaining_accounts_info: RemainingAccountsInfo,
}

#[derive(Accounts)]
pub struct ExecuteStrategyMeteoraDlmm<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut)]
    pub strategy: Box<Account<'info, Strategy>>,
    #[account(
        mut,
        associated_token::mint = token_x_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_x_program,
    )]
    pub vault_token_x: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = token_y_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_y_program,
    )]
    pub vault_token_y: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: validated against strategy in [handler] and in dlmm program
    #[account(mut)]
    pub position: UncheckedAccount<'info>,
    /// CHECK: validated in dlmm program
    #[account(mut)]
    pub lb_pair: UncheckedAccount<'info>,
    /// CHECK: validated in dlmm program
    #[account(mut)]
    pub bin_array_bitmap_extension: Option<UncheckedAccount<'info>>,
    /// CHECK: validated in dlmm program
    #[account(mut)]
    pub reserve_x: UncheckedAccount<'info>,
    /// CHECK: validated in dlmm program
    #[account(mut)]
    pub reserve_y: UncheckedAccount<'info>,
    pub token_x_mint: InterfaceAccount<'info, Mint>,
    pub token_y_mint: InterfaceAccount<'info, Mint>,
    pub token_x_program: Interface<'info, TokenInterface>,
    pub token_y_program: Interface<'info, TokenInterface>,
    /// CHECK: validated in dlmm program
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: DLMM Program
    #[account(address = dlmm::ID)]
    pub dlmm_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> ExecuteStrategyMeteoraDlmm<'info> {
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, ExecuteStrategyMeteoraDlmm<'info>>,
        params: ExecuteStrategyMeteoraDlmmParams,
    ) -> Result<()> {
        let ExecuteStrategyMeteoraDlmm {
            authority,
            config,
            vault,
            strategy,
            vault_token_x,
            vault_token_y,
            position,
            lb_pair,
            bin_array_bitmap_extension,
            reserve_x,
            reserve_y,
            token_x_mint,
            token_y_mint,
            token_x_program,
            token_y_program,
            event_authority,
            dlmm_program,
            ..
        } = ctx.accounts;

        let ExecuteStrategyMeteoraDlmmParams {
            liquidity_parameter,
            remaining_accounts_info,
        } = params;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        config.is_protocol_operational()?;

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_authority = vault.authority;
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_authority, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        let position_key = position.key();

        let strategy_key = strategy.key();
        let strategy: &mut Strategy = strategy.as_mut();
        let strategy_bump = strategy.bump;
        let strategy_seeds = strategy_seeds!(vault_key, position_key, strategy_bump);

        Strategy::validate_address(strategy_seeds, strategy_key)?;

        let StrategyType::MeteoraDlmm {
            position: strategy_position,
        } = strategy.strategy_type
        else {
            return err!(HedgeVaultError::InvalidStrategyType);
        };

        validate!(
            strategy_position == position_key,
            HedgeVaultError::InvalidPosition
        )?;

        let now = Clock::get()?.unix_timestamp;
        strategy.record_action(now);
        drop(vault);

        // token X and Y are deployed directly from vault token accounts, rebalancing is done via Jupiter swap beforehand
        add_liquidity_by_strategy2(
            CpiContext::new(
                dlmm_program.to_account_info(),
                AddLiquidityByStrategy2 {
                    position: position.to_account_info(),
                    lb_pair: lb_pair.to_account_info(),
                    bin_array_bitmap_extension: bin_array_bitmap_extension
                        .as_ref()
                        .map(|acc| acc.to_account_info()),
                    user_token_x: vault_token_x.to_account_info(),
                    user_token_y: vault_token_y.to_account_info(),
                    reserve_x: reserve_x.to_account_info(),
                    reserve_y: reserve_y.to_account_info(),
                    token_x_mint: token_x_mint.to_account_info(),
                    token_y_mint: token_y_mint.to_account_info(),
                    sender: vault_acc_info,
                    token_x_program: token_x_program.to_account_info(),
                    token_y_program: token_y_program.to_account_info(),
                    event_authority: event_authority.to_account_info(),
                    program: dlmm_program.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds])
            .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
            liquidity_parameter,
            remaining_accounts_info,
        )?;

        Ok(())
    }
}
