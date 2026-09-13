use anchor_lang::prelude::*;

use crate::{config_seeds, seeds::CONFIG, Config};

#[derive(Accounts)]
pub struct PauseProtocol<'info> {
    pub guardian: Signer<'info>,
    #[account(mut)]
    pub config: AccountLoader<'info, Config>,
}

impl<'info> PauseProtocol<'info> {
    pub fn handler(ctx: Context<PauseProtocol>) -> Result<()> {
        let PauseProtocol {
            guardian, config, ..
        } = ctx.accounts;

        let config_key = config.key();
        let config = &mut config.load_mut()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_guardian(guardian.key())?;

        config.pause();

        Ok(())
    }
}
