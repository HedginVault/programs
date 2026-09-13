use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    seeds::{CONFIG, MANAGER},
    Config, Manager, NewManagerArgs,
};

#[derive(Accounts)]
pub struct AddManager<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    /// CHECK: Manager authority being whitelisted
    pub authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = admin,
        space = Manager::DISCRIMINATOR.len() + Manager::INIT_SPACE,
        seeds = [MANAGER, authority.key().as_ref()],
        bump,
    )]
    pub manager: Account<'info, Manager>,
    pub system_program: Program<'info, System>,
}

impl<'info> AddManager<'info> {
    pub fn handler(ctx: Context<AddManager>) -> Result<()> {
        let AddManager {
            admin,
            config,
            authority,
            manager,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_admin(admin.key())?;

        manager.set_inner(Manager::new(NewManagerArgs {
            authority: authority.key(),
            bump: ctx.bumps.manager,
        }));

        Ok(())
    }
}
