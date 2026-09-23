use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::get_associated_token_address_with_program_id,
    token_2022::{close_account, CloseAccount},
    token_interface::TokenAccount,
};

use crate::{
    config_seeds, dlmm,
    error::HedgeVaultError,
    events::StrategyClosed,
    seeds::{CONFIG, STRATEGY, VAULT},
    strategy_seeds, validate, vault_seeds, Config, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct VaultCloseStrategy<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        mut,
        close = authority
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    pub system_program: Program<'info, System>,
}

impl<'info> VaultCloseStrategy<'info> {
    pub fn handler(ctx: Context<'_, '_, '_, 'info, VaultCloseStrategy<'info>>) -> Result<()> {
        let VaultCloseStrategy {
            authority,
            config,
            vault,
            strategy,
            ..
        } = ctx.accounts;

        let vault_acc_info = vault.to_account_info();

        let config_key = config.key();
        let config = config.load()?;
        let config_seeds = config_seeds!(config.bump);

        Config::validate_address(config_seeds, config_key)?;
        // closing a strategy is an exit path, it stays open while the protocol is reduce-only
        config.is_protocol_withdrawable()?;

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;
        vault.is_vault_withdrawable()?;

        // the strategy PDA is derived from the vault it belongs to, so another vault's
        // strategy record cannot be closed here
        let protocol_account = match strategy.strategy_type {
            StrategyType::MeteoraDlmm { position } => position,
            StrategyType::JupiterSwap { target_mint } => target_mint,
        };

        validate!(
            strategy.vault == vault_key,
            HedgeVaultError::InvalidStrategy
        )?;
        Strategy::validate_address(
            strategy_seeds!(vault_key, protocol_account, strategy.bump),
            strategy.key(),
        )?;

        // the Strategy account is closed by the `close` constraint on every path below
        vault.decrement_open_strategies()?;
        drop(vault);

        emit!(StrategyClosed {
            vault: vault_key,
            strategy: strategy.key(),
            id: strategy.id,
            strategy_type: strategy.strategy_type,
            created_ts: strategy.created_ts,
            closed_ts: Clock::get()?.unix_timestamp,
        });

        match strategy.strategy_type {
            StrategyType::MeteoraDlmm { position } => {
                // [0] - position account
                // [1] - dlmm_program
                // [2] - dlmm_event_authority
                let [position_account, dlmm_program, dlmm_event_authority] = ctx.remaining_accounts
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

                validate!(
                    token_program.key() == anchor_spl::token::ID
                        || token_program.key() == anchor_spl::token_2022::ID,
                    HedgeVaultError::InvalidTokenProgram
                )?;

                // only the vault's own ATA for the target mint may be closed, never an escrow
                validate!(
                    vault_target_mint_token_account.key()
                        == get_associated_token_address_with_program_id(
                            &vault_key,
                            &target_mint,
                            token_program.key,
                        ),
                    HedgeVaultError::InvalidTargetMintTokenAccount
                )?;

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
                validate!(
                    deserialized_token_account.owner == vault_key,
                    HedgeVaultError::InvalidTokenAccountOwner
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
