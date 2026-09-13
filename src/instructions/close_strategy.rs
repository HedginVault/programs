use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{close_account, CloseAccount},
    token_interface::TokenAccount,
};

use crate::{
    dlmm, error::HedgeVaultError, events::StrategyClosed, seeds::VAULT, validate, vault_seeds,
    Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct CloseStrategy<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        mut,
        close = authority
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    pub system_program: Program<'info, System>,
}

impl<'info> CloseStrategy<'info> {
    pub fn handler(ctx: Context<'_, '_, '_, 'info, CloseStrategy<'info>>) -> Result<()> {
        let CloseStrategy {
            authority,
            vault,
            strategy,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();
        let vault_key = vault.key();
        let vault = vault.load()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        drop(vault);

        emit!(StrategyClosed {
            vault: vault_key,
            strategy: strategy.key(),
        });

        match strategy.strategy_type {
            StrategyType::MeteoraDlmm { position } => {
                // [0] - position account
                // [1] - dlmm_program
                // [2] - dlmm_event_authority
                let [position_account, dlmm_program, dlmm_event_authority] =
                    ctx.remaining_accounts
                else {
                    return Err(HedgeVaultError::InvalidRemainingAccounts.into());
                };

                validate!(
                    position_account.key() == position,
                    HedgeVaultError::InvalidPosition
                )?;
                validate!(
                    dlmm_program.key() == dlmm::ID,
                    HedgeVaultError::InvalidProgramId
                )?;

                // fails in dlmm program if the position still holds liquidity
                dlmm::cpi::close_position2(CpiContext::new_with_signer(
                    dlmm_program.clone(),
                    dlmm::cpi::accounts::ClosePosition2 {
                        position: position_account.clone(),
                        sender: vault_acc_info,
                        rent_receiver: authority.to_account_info(),
                        event_authority: dlmm_event_authority.clone(),
                        program: dlmm_program.clone(),
                    },
                    &[vault_seeds],
                ))?;
            }
            StrategyType::JupiterSwap { target_mint } => {
                // [0] - vault_target_mint_token_account
                // [1] - token_program
                let [vault_target_mint_token_account, token_program] = ctx.remaining_accounts
                else {
                    return Err(HedgeVaultError::InvalidRemainingAccounts.into());
                };

                if vault_target_mint_token_account.get_lamports() == 0 {
                    // ATA does not exist, nothing to close
                    return Ok(());
                }

                let vault_target_mint_token_account_data =
                    vault_target_mint_token_account.data.borrow();
                let deserialized_token_account =
                    TokenAccount::try_deserialize(&mut &vault_target_mint_token_account_data[..])?;

                validate!(
                    deserialized_token_account.mint == target_mint,
                    HedgeVaultError::InvalidTargetMintTokenAccount
                )?;

                drop(vault_target_mint_token_account_data);

                // fails in token program if the account still holds a balance
                close_account(CpiContext::new_with_signer(
                    token_program.to_account_info(),
                    CloseAccount {
                        account: vault_target_mint_token_account.to_account_info(),
                        destination: authority.to_account_info(),
                        authority: vault_acc_info,
                    },
                    &[vault_seeds],
                ))?;
            }
        }

        Ok(())
    }
}
