use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use core::mem::size_of;

use crate::{
    error::HedgeVaultError, events::ConfigMigrated, seeds::CONFIG, validate, Config,
    CONFIG_V1_LEN,
};

/// Grows the deployed v1 config (160 bytes) to the current layout. Runs once.
#[derive(Accounts)]
pub struct MigrateConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    /// CHECK: v1 layout cannot be typed before it is resized, validated in [handler]
    #[account(
        mut,
        seeds = [CONFIG],
        bump,
    )]
    pub config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> MigrateConfig<'info> {
    pub fn handler(ctx: Context<MigrateConfig>) -> Result<()> {
        let MigrateConfig {
            admin,
            config,
            system_program,
            ..
        } = ctx.accounts;

        let config_info = config.to_account_info();

        {
            let data = config_info.try_borrow_data()?;

            validate!(
                config_info.owner == &crate::ID && data.starts_with(Config::DISCRIMINATOR),
                HedgeVaultError::InvalidConfig
            )?;
            validate!(
                data.len() == CONFIG_V1_LEN,
                HedgeVaultError::InvalidConfigVersion
            )?;

            let v1_admin = Pubkey::try_from(&data[8..40]).unwrap();
            validate!(v1_admin == admin.key(), HedgeVaultError::InvalidAdmin)?;
        }

        let new_len = Config::DISCRIMINATOR.len() + Config::INIT_SPACE;
        let rent_due = Rent::get()?
            .minimum_balance(new_len)
            .saturating_sub(config_info.lamports());

        if rent_due > 0 {
            transfer(
                CpiContext::new(
                    system_program.to_account_info(),
                    Transfer {
                        from: admin.to_account_info(),
                        to: config_info.clone(),
                    },
                ),
                rent_due,
            )?;
        }

        config_info.resize(new_len)?;

        let mut data = config_info.try_borrow_mut_data()?;
        let config: &mut Config = bytemuck::from_bytes_mut(
            &mut data[Config::DISCRIMINATOR.len()..Config::DISCRIMINATOR.len() + size_of::<Config>()],
        );
        config.migrate();

        emit!(ConfigMigrated {
            version: config.version,
        });

        Ok(())
    }
}
