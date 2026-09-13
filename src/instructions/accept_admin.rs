use anchor_lang::prelude::*;

use crate::{config_seeds, events::AdminAccepted, seeds::CONFIG, Config};

/// Completes the two-step admin transfer started by `update_config`.
#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub pending_admin: Signer<'info>,
    #[account(mut)]
    pub config: AccountLoader<'info, Config>,
}

impl<'info> AcceptAdmin<'info> {
    pub fn handler(ctx: Context<AcceptAdmin>) -> Result<()> {
        let AcceptAdmin {
            pending_admin,
            config,
        } = ctx.accounts;

        let config_key = config.key();
        let config = &mut config.load_mut()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;

        let previous_admin = config.admin;
        config.accept_admin(pending_admin.key())?;

        emit!(AdminAccepted {
            previous_admin,
            admin: config.admin,
        });

        Ok(())
    }
}
