use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::{
    config_seeds,
    dlmm::{
        self,
        accounts::PositionV2,
        cpi::{accounts::ClaimFee2, claim_fee2},
        types::RemainingAccountsInfo,
    },
    error::HedgeVaultError,
    events::MeteoraDlmmFeeClaimed,
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, SafeConvert, SafeMath, Strategy, StrategyType,
    Vault, MAX_BPS, TREASURY_CLAIM_FEE_BPS,
};

#[derive(Accounts)]
pub struct MeteoraDlmmClaimFee<'info> {
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
    pub vault_token_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
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

impl<'info> MeteoraDlmmClaimFee<'info> {
    pub fn handler(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmClaimFee<'info>>,
        remaining_accounts_info: RemainingAccountsInfo,
    ) -> Result<()> {
        let MeteoraDlmmClaimFee {
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
        config.is_protocol_operational()?;
        config.validate_treasury_authority(treasury_authority.key())?;

        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.is_vault_withdrawable()?;

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

        let position_acc_info = position.to_account_info();
        let position = position.load()?;
        let lower_bin_id = position.lower_bin_id;
        let upper_bin_id = position.upper_bin_id;
        drop(position);

        let balance_x_before = vault_token_x.amount;
        let balance_y_before = vault_token_y.amount;

        // claim all position fees
        claim_fee2(
            CpiContext::new(
                dlmm_program.to_account_info(),
                ClaimFee2 {
                    lb_pair: lb_pair.to_account_info(),
                    position: position_acc_info,
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

        // measured by balance change so token transfer fees are accounted for
        vault_token_x.reload()?;
        vault_token_y.reload()?;
        let amount_x = vault_token_x.amount.safe_sub(balance_x_before)?;
        let amount_y = vault_token_y.amount.safe_sub(balance_y_before)?;

        let treasury_amount_x = transfer_treasury_fee(
            amount_x,
            vault_token_x,
            treasury_token_x,
            token_x_mint,
            token_x_program,
            vault_acc_info.clone(),
            vault_seeds,
        )?;
        let treasury_amount_y = transfer_treasury_fee(
            amount_y,
            vault_token_y,
            treasury_token_y,
            token_y_mint,
            token_y_program,
            vault_acc_info,
            vault_seeds,
        )?;

        emit!(MeteoraDlmmFeeClaimed {
            vault: vault_key,
            strategy: strategy_key,
            position: position_key,
            amount_x,
            amount_y,
            treasury_amount_x,
            treasury_amount_y,
            strategy_id: strategy.id,
            token_x_mint: token_x_mint.key(),
            token_y_mint: token_y_mint.key(),
            vault_retained_x: amount_x.safe_sub(treasury_amount_x)?,
            vault_retained_y: amount_y.safe_sub(treasury_amount_y)?,
        });

        Ok(())
    }
}

/// Sends [TREASURY_CLAIM_FEE_BPS] of a claimed amount from the vault to the treasury, returns the amount sent.
fn transfer_treasury_fee<'info>(
    claimed_amount: u64,
    vault_token_account: &InterfaceAccount<'info, TokenAccount>,
    treasury_token_account: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    vault: AccountInfo<'info>,
    vault_seeds: &[&[u8]],
) -> Result<u64> {
    let amount = (claimed_amount as u128)
        .safe_mul(TREASURY_CLAIM_FEE_BPS as u128)?
        .safe_div(MAX_BPS as u128)?
        .safe_to_u64()?;

    if amount > 0 {
        transfer_checked(
            CpiContext::new(
                token_program.to_account_info(),
                TransferChecked {
                    from: vault_token_account.to_account_info(),
                    mint: mint.to_account_info(),
                    to: treasury_token_account.to_account_info(),
                    authority: vault,
                },
            )
            .with_signer(&[vault_seeds]),
            amount,
            mint.decimals,
        )?;
    }

    Ok(amount)
}
