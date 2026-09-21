use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    config_seeds,
    dlmm::{
        self,
        accounts::PositionV2,
        cpi::{
            accounts::{ClaimFee2, ClosePosition2, RemoveLiquidityByRange2},
            claim_fee2, close_position2, remove_liquidity_by_range2,
        },
        types::RemainingAccountsInfo,
    },
    error::HedgeVaultError,
    events::MeteoraDlmmPositionClosed,
    instructions::meteora_dlmm_claim_fee::transfer_treasury_fee,
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, SafeMath, Strategy, StrategyType, Vault,
    MAX_BPS,
};

#[derive(Accounts)]
pub struct MeteoraDlmmClosePosition<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(mut, close = authority)]
    pub strategy: Box<Account<'info, Strategy>>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = token_x_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_x_program,
    )]
    pub vault_token_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = token_y_mint,
        associated_token::authority = vault,
        associated_token::token_program = token_y_program,
    )]
    pub vault_token_y: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: validated against config in [handler]
    pub treasury_authority: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = token_x_mint,
        associated_token::authority = treasury_authority,
        associated_token::token_program = token_x_program,
    )]
    pub treasury_token_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = token_y_mint,
        associated_token::authority = treasury_authority,
        associated_token::token_program = token_y_program,
    )]
    pub treasury_token_y: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: validated against strategy in [handler] and in dlmm program
    #[account(mut)]
    pub position: AccountLoader<'info, PositionV2>,
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
    pub token_x_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_y_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_x_program: Interface<'info, TokenInterface>,
    pub token_y_program: Interface<'info, TokenInterface>,
    /// CHECK: validated in dlmm program
    pub memo_program: UncheckedAccount<'info>,
    /// CHECK: validated in dlmm program
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: DLMM Program
    #[account(address = dlmm::ID)]
    pub dlmm_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> MeteoraDlmmClosePosition<'info> {
    /// Drains all liquidity and fees out of a Meteora DLMM position and closes it in one
    /// instruction, so callers do not need to remove liquidity and claim fees first.
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmClosePosition<'info>>,
        remaining_accounts_info: RemainingAccountsInfo,
    ) -> Result<()> {
        let MeteoraDlmmClosePosition {
            authority,
            config,
            vault,
            strategy,
            vault_token_x,
            vault_token_y,
            treasury_authority,
            treasury_token_x,
            treasury_token_y,
            position,
            lb_pair,
            bin_array_bitmap_extension,
            reserve_x,
            reserve_y,
            token_x_mint,
            token_y_mint,
            token_x_program,
            token_y_program,
            memo_program,
            event_authority,
            dlmm_program,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        // closing a position is an exit path, it stays open while the protocol is reduce-only
        config.is_protocol_withdrawable()?;
        config.validate_treasury_authority(treasury_authority.key())?;

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.is_vault_withdrawable()?;

        let position_key = position.key();

        let strategy_key = strategy.key();
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

        vault.decrement_open_strategies()?;
        drop(vault);

        let position_acc_info = position.to_account_info();
        let position_data = position.load()?;
        let lower_bin_id = position_data.lower_bin_id;
        let upper_bin_id = position_data.upper_bin_id;
        drop(position_data);

        // remove 100% of the position's liquidity, no treasury share is taken from principal
        let balance_x_before_remove = vault_token_x.amount;
        let balance_y_before_remove = vault_token_y.amount;

        remove_liquidity_by_range2(
            CpiContext::new(
                dlmm_program.to_account_info(),
                RemoveLiquidityByRange2 {
                    position: position_acc_info.clone(),
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
                    sender: vault_acc_info.clone(),
                    token_x_program: token_x_program.to_account_info(),
                    token_y_program: token_y_program.to_account_info(),
                    memo_program: memo_program.to_account_info(),
                    event_authority: event_authority.to_account_info(),
                    program: dlmm_program.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds])
            .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
            lower_bin_id,
            upper_bin_id,
            MAX_BPS,
            remaining_accounts_info.clone(),
        )?;

        vault_token_x.reload()?;
        vault_token_y.reload()?;
        let liquidity_amount_x = vault_token_x.amount.safe_sub(balance_x_before_remove)?;
        let liquidity_amount_y = vault_token_y.amount.safe_sub(balance_y_before_remove)?;

        // claim whatever fees the position accrued, treasury takes its usual cut of fees only
        let balance_x_before_claim = vault_token_x.amount;
        let balance_y_before_claim = vault_token_y.amount;

        claim_fee2(
            CpiContext::new(
                dlmm_program.to_account_info(),
                ClaimFee2 {
                    lb_pair: lb_pair.to_account_info(),
                    position: position_acc_info.clone(),
                    sender: vault_acc_info.clone(),
                    reserve_x: reserve_x.to_account_info(),
                    reserve_y: reserve_y.to_account_info(),
                    user_token_x: vault_token_x.to_account_info(),
                    user_token_y: vault_token_y.to_account_info(),
                    token_x_mint: token_x_mint.to_account_info(),
                    token_y_mint: token_y_mint.to_account_info(),
                    token_program_x: token_x_program.to_account_info(),
                    token_program_y: token_y_program.to_account_info(),
                    memo_program: memo_program.to_account_info(),
                    event_authority: event_authority.to_account_info(),
                    program: dlmm_program.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds])
            .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
            lower_bin_id,
            upper_bin_id,
            remaining_accounts_info,
        )?;

        vault_token_x.reload()?;
        vault_token_y.reload()?;
        let fee_amount_x = vault_token_x.amount.safe_sub(balance_x_before_claim)?;
        let fee_amount_y = vault_token_y.amount.safe_sub(balance_y_before_claim)?;

        let treasury_amount_x = transfer_treasury_fee(
            fee_amount_x,
            vault_token_x,
            treasury_token_x,
            token_x_mint,
            token_x_program,
            vault_acc_info.clone(),
            vault_seeds,
        )?;
        let treasury_amount_y = transfer_treasury_fee(
            fee_amount_y,
            vault_token_y,
            treasury_token_y,
            token_y_mint,
            token_y_program,
            vault_acc_info.clone(),
            vault_seeds,
        )?;

        // fails in dlmm program if the position still holds liquidity, which it should not by now
        close_position2(CpiContext::new_with_signer(
            dlmm_program.to_account_info(),
            ClosePosition2 {
                position: position_acc_info,
                sender: vault_acc_info,
                rent_receiver: authority.to_account_info(),
                event_authority: event_authority.to_account_info(),
                program: dlmm_program.to_account_info(),
            },
            &[vault_seeds],
        ))?;

        emit!(MeteoraDlmmPositionClosed {
            vault: vault_key,
            strategy: strategy_key,
            position: position_key,
            liquidity_amount_x,
            liquidity_amount_y,
            fee_amount_x,
            fee_amount_y,
            treasury_amount_x,
            treasury_amount_y,
        });

        Ok(())
    }
}
