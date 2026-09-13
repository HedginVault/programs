use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::{
    events::StrategyInitialized,
    seeds::{STRATEGY, VAULT},
    vault_seeds, NewStrategyArgs, Strategy, StrategyType, Vault,
};

#[derive(Accounts)]
pub struct InitializeStrategyJupiterSwap<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
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

impl<'info> InitializeStrategyJupiterSwap<'info> {
    pub fn handler(ctx: Context<InitializeStrategyJupiterSwap>) -> Result<()> {
        let InitializeStrategyJupiterSwap {
            authority,
            vault,
            strategy,
            destination_mint,
            ..
        } = ctx.accounts;

        let vault_key = vault.key();
        let mut vault = vault.load_mut()?;
        let vault_id = vault.id.to_le_bytes();
        let vault_authority = vault.authority.key();
        let vault_bump = vault.bump;
        let vault_seeds = vault_seeds!(vault_id, vault_authority, vault_bump);

        Vault::validate_address(vault_seeds, vault_key)?;
        vault.validate_authority(authority.key())?;

        // no actions required, execute will create the vault ATA for destination_mint if needed

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

        emit!(StrategyInitialized {
            vault: vault_key,
            strategy: strategy.key(),
            id: strategy.id,
            strategy_type: strategy.strategy_type,
        });

        Ok(())
    }
}
