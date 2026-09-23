use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    dlmm::{
        cpi::{accounts::InitializePosition2, initialize_position2},
        ID as dlmm_ID,
    },
    error::HedgeVaultError,
    events::StrategyInitialized,
    seeds::{CONFIG, STRATEGY, VAULT},
    vault_seeds, Config, NewStrategyArgs, Strategy, StrategyType, Vault,
};

fn initial_width(lower_bin_id: i32, upper_bin_id: i32) -> Result<i32> {
    let width = upper_bin_id
        .checked_sub(lower_bin_id)
        .ok_or(HedgeVaultError::InvalidPositionBinRange)?;
    require!(
        (1..=70).contains(&width),
        HedgeVaultError::InvalidPositionBinRange
    );
    Ok(width)
}

#[derive(Accounts)]
pub struct MeteoraDlmmInitializePosition<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        init,
        payer = authority,
        space = Strategy::DISCRIMINATOR.len() + Strategy::INIT_SPACE + StrategyType::space(&StrategyType::MeteoraDlmm { position: position.key() }),
        seeds = [STRATEGY, vault.key().as_ref(), position.key().as_ref()],
        bump,
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    /// CHECK: Meteora DLMM Position, initialized by CPI
    #[account(mut)]
    pub position: Signer<'info>,
    /// CHECK: Meteora DLMM Lb Pair, validated in dlmm program
    #[account(mut)]
    pub lb_pair: UncheckedAccount<'info>,
    /// CHECK: Meteora DLMM event authority, validated in dlmm program
    pub event_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: Meteora DLMM Program
    #[account(address = dlmm_ID)]
    pub dlmm_program: UncheckedAccount<'info>,
}

impl<'info> MeteoraDlmmInitializePosition<'info> {
    pub fn handler(
        ctx: Context<MeteoraDlmmInitializePosition>,
        lower_bin_id: i32,
        upper_bin_id: i32,
    ) -> Result<()> {
        let MeteoraDlmmInitializePosition {
            authority,
            config,
            vault,
            strategy,
            position,
            lb_pair,
            event_authority,
            system_program,
            dlmm_program,
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

        let now = Clock::get()?.unix_timestamp;

        strategy.set_inner(Strategy::new(NewStrategyArgs {
            vault: vault_key,
            id: vault.next_strategy_id,
            bump: ctx.bumps.strategy,
            created_ts: now,
            strategy_type: StrategyType::MeteoraDlmm {
                position: position.key(),
            },
        }));

        vault.increment_strategy_id()?;
        vault.increment_open_strategies()?;
        drop(vault);

        emit!(StrategyInitialized {
            vault: vault_key,
            strategy: strategy.key(),
            id: strategy.id,
            strategy_type: strategy.strategy_type,
        });

        // vault is the owner of the position
        let width = initial_width(lower_bin_id, upper_bin_id)?;
        initialize_position2(
            CpiContext::new(
                dlmm_program.to_account_info(),
                InitializePosition2 {
                    payer: authority.to_account_info(),
                    position: position.to_account_info(),
                    lb_pair: lb_pair.to_account_info(),
                    owner: vault_acc_info,
                    system_program: system_program.to_account_info(),
                    event_authority: event_authority.to_account_info(),
                    program: dlmm_program.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds]),
            lower_bin_id,
            width,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_position_respects_meteora_limit() {
        assert_eq!(initial_width(-35, 35).unwrap(), 70);
        for (lower, upper) in [(0, 0), (0, 71), (i32::MIN, i32::MAX)] {
            assert!(initial_width(lower, upper).is_err());
        }
    }
}
