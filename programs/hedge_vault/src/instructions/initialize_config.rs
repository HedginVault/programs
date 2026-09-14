use anchor_lang::prelude::*;

use crate::{
    error::HedgeVaultError, events::ConfigInitialized, seeds::CONFIG, validate, Config,
    NewConfigArgs, MAX_BPS,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitializeConfigArgs {
    pub nav_updater: Pubkey,
    pub treasury_authority: Pubkey,
    pub guardian: Pubkey,
    pub platform_performance_fee_bps: u16,
    pub platform_management_fee_bps: u16,
    pub max_nav_deviation_bps: u16,
    pub max_epoch_outflow_bps: u16,
    pub max_slippage_bps: u16,
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        payer = admin,
        seeds = [CONFIG],
        bump
    )]
    pub config: AccountLoader<'info, Config>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeConfig<'info> {
    pub fn handler(ctx: Context<InitializeConfig>, args: InitializeConfigArgs) -> Result<()> {
        let InitializeConfig { admin, config, .. } = ctx.accounts;

        validate!(
            args.nav_updater != Pubkey::default(),
            HedgeVaultError::InvalidPubkey
        )?;
        validate!(
            args.treasury_authority != Pubkey::default(),
            HedgeVaultError::InvalidPubkey
        )?;
        validate!(args.guardian != Pubkey::default(), HedgeVaultError::InvalidPubkey)?;

        for bps in [
            args.platform_performance_fee_bps,
            args.platform_management_fee_bps,
            args.max_nav_deviation_bps,
            args.max_epoch_outflow_bps,
        ] {
            validate!(bps <= MAX_BPS, HedgeVaultError::InvalidBasisPoints)?;
        }

        validate!(
            args.max_slippage_bps > 0 && args.max_slippage_bps <= MAX_BPS,
            HedgeVaultError::InvalidBasisPoints
        )?;

        let mut config = config.load_init()?;

        *config = Config::new(NewConfigArgs {
            admin: admin.key(),
            nav_updater: args.nav_updater,
            treasury_authority: args.treasury_authority,
            guardian: args.guardian,
            platform_performance_fee_bps: args.platform_performance_fee_bps,
            platform_management_fee_bps: args.platform_management_fee_bps,
            max_nav_deviation_bps: args.max_nav_deviation_bps,
            max_epoch_outflow_bps: args.max_epoch_outflow_bps,
            max_slippage_bps: args.max_slippage_bps,
            bump: ctx.bumps.config,
        });

        emit!(ConfigInitialized {
            admin: config.admin,
            nav_updater: config.nav_updater,
            treasury_authority: config.treasury_authority,
            guardian: config.guardian,
        });

        Ok(())
    }
}
