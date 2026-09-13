use anchor_lang::prelude::*;

use crate::{
    config_seeds, error::HedgeVaultError, events::ConfigUpdated, seeds::CONFIG, validate,
    Config, ProtocolStatus, MAX_BPS,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateConfigArgs {
    pub new_admin: Option<Pubkey>,
    pub nav_updater: Option<Pubkey>,
    pub treasury_authority: Option<Pubkey>,
    pub guardian: Option<Pubkey>,
    pub platform_performance_fee_bps: Option<u16>,
    pub platform_management_fee_bps: Option<u16>,
    pub max_nav_deviation_bps: Option<u16>,
    pub max_epoch_outflow_bps: Option<u16>,
    pub status: Option<ProtocolStatus>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub admin: Signer<'info>,
    #[account(mut)]
    pub config: AccountLoader<'info, Config>,
}

impl<'info> UpdateConfig<'info> {
    pub fn handler(ctx: Context<UpdateConfig>, args: UpdateConfigArgs) -> Result<()> {
        let UpdateConfig { admin, config, .. } = ctx.accounts;

        let config_key = config.key();
        let config = &mut config.load_mut()?;
        let config_bump = config.bump;
        let config_seeds = config_seeds!(config_bump);

        Config::validate_address(config_seeds, config_key)?;
        config.validate_admin(admin.key())?;

        if let Some(new_admin) = args.new_admin {
            validate!(new_admin != Pubkey::default(), HedgeVaultError::InvalidPubkey)?;

            config.admin = new_admin;
        }

        if let Some(nav_updater) = args.nav_updater {
            validate!(
                nav_updater != Pubkey::default(),
                HedgeVaultError::InvalidPubkey
            )?;

            config.nav_updater = nav_updater;
        }

        if let Some(treasury_authority) = args.treasury_authority {
            validate!(
                treasury_authority != Pubkey::default(),
                HedgeVaultError::InvalidPubkey
            )?;

            config.treasury_authority = treasury_authority;
        }

        if let Some(guardian) = args.guardian {
            validate!(guardian != Pubkey::default(), HedgeVaultError::InvalidPubkey)?;

            config.guardian = guardian;
        }

        if let Some(platform_performance_fee_bps) = args.platform_performance_fee_bps {
            validate!(
                platform_performance_fee_bps <= MAX_BPS,
                HedgeVaultError::InvalidBasisPoints
            )?;

            config.platform_performance_fee_bps = platform_performance_fee_bps;
        }

        if let Some(platform_management_fee_bps) = args.platform_management_fee_bps {
            validate!(
                platform_management_fee_bps <= MAX_BPS,
                HedgeVaultError::InvalidBasisPoints
            )?;

            config.platform_management_fee_bps = platform_management_fee_bps;
        }

        if let Some(max_nav_deviation_bps) = args.max_nav_deviation_bps {
            validate!(
                max_nav_deviation_bps <= MAX_BPS,
                HedgeVaultError::InvalidBasisPoints
            )?;

            config.max_nav_deviation_bps = max_nav_deviation_bps;
        }

        if let Some(max_epoch_outflow_bps) = args.max_epoch_outflow_bps {
            validate!(
                max_epoch_outflow_bps <= MAX_BPS,
                HedgeVaultError::InvalidBasisPoints
            )?;

            config.max_epoch_outflow_bps = max_epoch_outflow_bps;
        }

        if let Some(status) = args.status {
            config.status = status;
        }

        emit!(ConfigUpdated {
            admin: config.admin,
            nav_updater: config.nav_updater,
            treasury_authority: config.treasury_authority,
            guardian: config.guardian,
            platform_performance_fee_bps: config.platform_performance_fee_bps,
            platform_management_fee_bps: config.platform_management_fee_bps,
            max_nav_deviation_bps: config.max_nav_deviation_bps,
            max_epoch_outflow_bps: config.max_epoch_outflow_bps,
            status: config.status,
        });

        Ok(())
    }
}
