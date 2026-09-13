use anchor_lang::prelude::*;

use crate::{
    config_seeds,
    events::ManagerRemoved,
    manager_seeds,
    seeds::{CONFIG, MANAGER},
    Config, Manager,
};

#[derive(Accounts)]
pub struct RemoveManager<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub config: AccountLoader<'info, Config>,
    #[account(
        mut,
        close = admin,
    )]
    pub manager: Account<'info, Manager>,
    pub system_program: Program<'info, System>,
}

impl<'info> RemoveManager<'info> {
    pub fn handler(ctx: Context<RemoveManager>) -> Result<()> {
        let RemoveManager {
            admin,
            config,
            manager,
            ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = config.load()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_admin(admin.key())?;

        let manager_key = manager.key();
        let manager_authority = manager.authority;
        let manager_bump = manager.bump;
        let manager_seeds = manager_seeds!(manager_authority, manager_bump);

        Manager::validate_address(manager_seeds, manager_key)?;

        emit!(ManagerRemoved {
            authority: manager_authority,
        });

        Ok(())
    }
}
