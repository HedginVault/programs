use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::{
    config_seeds,
    error::HedgeVaultError,
    events::StrategyInitialized,
    seeds::{CONFIG, STRATEGY, VAULT},
    validate, vault_seeds, Config, NewStrategyArgs, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct JupiterInitializeStrategy<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(mut)]
    pub vault: AccountLoader<'info, Vault>,
    #[account(
        init,
        payer = authority,
        space = Strategy::DISCRIMINATOR.len() + Strategy::INIT_SPACE + StrategyType::space(&StrategyType::JupiterSwap { target_mint: destination_mint.key() }),
        seeds = [STRATEGY, vault.key().as_ref(), destination_mint.key().as_ref()],
        bump,
    )]
    pub strategy: Box<Account<'info, Strategy>>,
    pub destination_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

impl<'info> JupiterInitializeStrategy<'info> {
    pub fn handler(ctx: Context<JupiterInitializeStrategy>) -> Result<()> {
        let JupiterInitializeStrategy {
            authority,
            config,
            vault,
            strategy,
            destination_mint,
            ..
        } = ctx.accounts;

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

        // vault_close_strategy closes the vault ATA of the target mint, which must never be the
        // deposit mint account or the share mint the vault cannot recreate
        validate!(
            destination_mint.key() != vault.deposit_mint
                && destination_mint.key() != vault.share_mint,
            HedgeVaultError::InvalidStrategyMint
        )?;

        // no actions required, jupiter_swap will create the vault ATA for destination_mint if needed

        let now = Clock::get()?.unix_timestamp;

        strategy.set_inner(Strategy::new(NewStrategyArgs {
            vault: vault_key,
            id: vault.next_strategy_id,
            bump: ctx.bumps.strategy,
            created_ts: now,
            strategy_type: StrategyType::JupiterSwap {
                target_mint: destination_mint.key(),
            },
        }));

        vault.increment_strategy_id()?;
        vault.increment_open_strategies()?;

        emit!(StrategyInitialized {
            vault: vault_key,
            strategy: strategy.key(),
            id: strategy.id,
            strategy_type: strategy.strategy_type,
        });

        Ok(())
    }
}
