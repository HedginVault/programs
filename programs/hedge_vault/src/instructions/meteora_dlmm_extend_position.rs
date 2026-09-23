use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    dlmm::{
        self,
        accounts::PositionV2,
        cpi::{accounts::IncreasePositionLength, increase_position_length},
    },
    error::HedgeVaultError,
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, vault_seeds, Config, Strategy, StrategyType, Vault,
};

const MAX_POSITION_WIDTH: i32 = 1_400;
// Meteora's MAX_RESIZE_LENGTH; each CPI reallocates less than Solana's 10 KiB limit.
const MAX_RESIZE_LENGTH: u16 = 91;
const UPPER_SIDE: u8 = 1;

fn validate_extension(lower: i32, upper: i32, bins_to_add: u16) -> Result<()> {
    require!(
        (1..=MAX_RESIZE_LENGTH).contains(&bins_to_add),
        HedgeVaultError::InvalidPositionBinRange
    );
    let width = upper
        .checked_sub(lower)
        .and_then(|span| span.checked_add(1))
        .ok_or(HedgeVaultError::InvalidPositionBinRange)?;
    require!(
        (1..=MAX_POSITION_WIDTH).contains(&width)
            && width
                .checked_add(i32::from(bins_to_add))
                .is_some_and(|new_width| new_width <= MAX_POSITION_WIDTH),
        HedgeVaultError::InvalidPositionBinRange
    );
    Ok(())
}

#[derive(Accounts)]
pub struct MeteoraDlmmExtendPosition<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        seeds = [STRATEGY, vault.key().as_ref(), position.key().as_ref()],
        bump = strategy.bump,
    )]
    pub strategy: Account<'info, Strategy>,
    #[account(mut)]
    pub position: AccountLoader<'info, PositionV2>,
    /// CHECK: checked against PositionV2 and by Meteora's CPI.
    pub lb_pair: UncheckedAccount<'info>,
    /// CHECK: validated by Meteora's CPI.
    pub event_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: constrained to the Meteora program.
    #[account(address = dlmm::ID)]
    pub dlmm_program: UncheckedAccount<'info>,
}

impl<'info> MeteoraDlmmExtendPosition<'info> {
    pub fn handler(ctx: Context<MeteoraDlmmExtendPosition>, bins_to_add: u16) -> Result<()> {
        let accounts = ctx.accounts;
        let config_key = accounts.config.key();
        let config = accounts.config.load()?;
        Config::validate_address(config_seeds!(config.bump), config_key)?;
        config.is_protocol_operational()?;

        let vault_key = accounts.vault.key();
        let vault = accounts.vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_seeds = vault_seeds!(vault_id, vault.bump);
        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(accounts.authority.key())?;
        vault.is_vault_operational()?;

        Strategy::validate_address(
            strategy_seeds!(vault_key, accounts.position.key(), accounts.strategy.bump),
            accounts.strategy.key(),
        )?;
        require!(
            matches!(accounts.strategy.strategy_type, StrategyType::MeteoraDlmm { position } if position == accounts.position.key()),
            HedgeVaultError::InvalidPosition
        );

        let position = accounts.position.load()?;
        require_keys_eq!(position.owner, vault_key, HedgeVaultError::InvalidPosition);
        require_keys_eq!(
            position.lb_pair,
            accounts.lb_pair.key(),
            HedgeVaultError::InvalidPosition
        );
        validate_extension(position.lower_bin_id, position.upper_bin_id, bins_to_add)?;
        drop(position);
        drop(vault);

        increase_position_length(
            CpiContext::new(
                accounts.dlmm_program.to_account_info(),
                IncreasePositionLength {
                    funder: accounts.authority.to_account_info(),
                    lb_pair: accounts.lb_pair.to_account_info(),
                    position: accounts.position.to_account_info(),
                    owner: accounts.vault.to_account_info(),
                    system_program: accounts.system_program.to_account_info(),
                    event_authority: accounts.event_authority.to_account_info(),
                    program: accounts.dlmm_program.to_account_info(),
                },
            )
            .with_signer(&[vault_seeds]),
            bins_to_add,
            UPPER_SIDE,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_steps_through_maximum_width() {
        assert!(validate_extension(-5, 64, 91).is_ok());
        assert!(validate_extension(-5, 1303, 91).is_ok());
        assert!(validate_extension(-5, 1393, 1).is_ok());
    }

    #[test]
    fn rejects_invalid_step_or_total_width() {
        for (lower, upper, add) in [
            (0, 69, 0),
            (0, 69, 92),
            (0, 1399, 1),
            (0, -1, 1),
            (i32::MIN, i32::MAX, 1),
        ] {
            assert!(validate_extension(lower, upper, add).is_err());
        }
    }
}
