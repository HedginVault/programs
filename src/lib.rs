pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod protocol;
pub mod state;
pub mod utils;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;
pub use utils::*;

declare_program!(jupiter);
declare_program!(dlmm);
declare_id!("r2ahBQ6gbPCJ9FxBymYcXuwXi8NmenRry7SE7QR7FAt");

#[program]
pub mod hedge_vault {
    use super::*;

    // Admin only

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        args: InitializeConfigArgs,
    ) -> Result<()> {
        InitializeConfig::handler(ctx, args)
    }

    pub fn update_config(ctx: Context<UpdateConfig>, args: UpdateConfigArgs) -> Result<()> {
        UpdateConfig::handler(ctx, args)
    }

    pub fn add_manager(ctx: Context<AddManager>) -> Result<()> {
        AddManager::handler(ctx)
    }

    pub fn remove_manager(ctx: Context<RemoveManager>) -> Result<()> {
        RemoveManager::handler(ctx)
    }

    pub fn claim_platform_fee(ctx: Context<ClaimPlatformFee>) -> Result<()> {
        ClaimPlatformFee::handler(ctx)
    }

    pub fn migrate_config(ctx: Context<MigrateConfig>) -> Result<()> {
        MigrateConfig::handler(ctx)
    }

    pub fn override_nav(ctx: Context<OverrideNav>, total_assets: u64) -> Result<()> {
        OverrideNav::handler(ctx, total_assets)
    }

    // Guardian

    pub fn pause_protocol(ctx: Context<PauseProtocol>) -> Result<()> {
        PauseProtocol::handler(ctx)
    }

    // Pending admin

    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        AcceptAdmin::handler(ctx)
    }

    // NAV Updater

    pub fn update_nav(ctx: Context<UpdateNav>, total_assets: u64) -> Result<()> {
        UpdateNav::handler(ctx, total_assets)
    }

    // Vault Manager

    pub fn initialize_vault(
        ctx: Context<InitializeVault>,
        args: InitializeVaultArgs,
    ) -> Result<()> {
        InitializeVault::handler(ctx, args)
    }

    pub fn update_vault(ctx: Context<UpdateVault>, args: UpdateVaultArgs) -> Result<()> {
        UpdateVault::handler(ctx, args)
    }

    pub fn close_vault(ctx: Context<CloseVault>) -> Result<()> {
        CloseVault::handler(ctx)
    }

    pub fn close_strategy<'info>(
        ctx: Context<'_, '_, '_, 'info, CloseStrategy<'info>>,
    ) -> Result<()> {
        CloseStrategy::handler(ctx)
    }

    pub fn claim_manager_fee(ctx: Context<ClaimManagerFee>) -> Result<()> {
        ClaimManagerFee::handler(ctx)
    }

    // User

    pub fn request_deposit(ctx: Context<RequestDeposit>, amount: u64) -> Result<()> {
        RequestDeposit::handler(ctx, amount)
    }

    pub fn cancel_deposit_request(ctx: Context<CancelDepositRequest>) -> Result<()> {
        CancelDepositRequest::handler(ctx)
    }

    pub fn resolve_deposit_request(ctx: Context<ResolveDepositRequest>) -> Result<()> {
        ResolveDepositRequest::handler(ctx)
    }

    pub fn request_withdrawal(ctx: Context<RequestWithdrawal>, shares: u64) -> Result<()> {
        RequestWithdrawal::handler(ctx, shares)
    }

    pub fn cancel_withdrawal_request(ctx: Context<CancelWithdrawalRequest>) -> Result<()> {
        CancelWithdrawalRequest::handler(ctx)
    }

    pub fn resolve_withdrawal_request(ctx: Context<ResolveWithdrawalRequest>) -> Result<()> {
        ResolveWithdrawalRequest::handler(ctx)
    }

    // Jupiter

    pub fn jupiter_initialize_strategy(ctx: Context<JupiterInitializeStrategy>) -> Result<()> {
        JupiterInitializeStrategy::handler(ctx)
    }

    pub fn jupiter_swap<'info>(
        ctx: Context<'_, '_, '_, 'info, JupiterSwap<'info>>,
        swap_data: Vec<u8>,
        amount: u64,
        slippage_bps: u16,
    ) -> Result<()> {
        JupiterSwap::handler(ctx, swap_data, amount, slippage_bps)
    }

    // Meteora DLMM

    pub fn meteora_dlmm_initialize_position(
        ctx: Context<MeteoraDlmmInitializePosition>,
        lower_bin_id: i32,
        upper_bin_id: i32,
    ) -> Result<()> {
        MeteoraDlmmInitializePosition::handler(ctx, lower_bin_id, upper_bin_id)
    }

    pub fn meteora_dlmm_add_liquidity<'info>(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmAddLiquidity<'info>>,
        params: MeteoraDlmmAddLiquidityParams,
    ) -> Result<()> {
        MeteoraDlmmAddLiquidity::handler(ctx, params)
    }

    pub fn meteora_dlmm_remove_liquidity<'info>(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmRemoveLiquidity<'info>>,
        params: MeteoraDlmmRemoveLiquidityParams,
    ) -> Result<()> {
        MeteoraDlmmRemoveLiquidity::handler(ctx, params)
    }

    pub fn meteora_dlmm_claim_fee<'info>(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmClaimFee<'info>>,
        remaining_accounts_info: dlmm::types::RemainingAccountsInfo,
    ) -> Result<()> {
        MeteoraDlmmClaimFee::handler(ctx, remaining_accounts_info)
    }
}
