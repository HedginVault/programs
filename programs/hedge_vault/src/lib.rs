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

    pub fn config_initialize(
        ctx: Context<ConfigInitialize>,
        args: ConfigInitializeArgs,
    ) -> Result<()> {
        ConfigInitialize::handler(ctx, args)
    }

    pub fn config_update(ctx: Context<ConfigUpdate>, args: ConfigUpdateArgs) -> Result<()> {
        ConfigUpdate::handler(ctx, args)
    }

    pub fn config_add_manager(ctx: Context<ConfigAddManager>) -> Result<()> {
        ConfigAddManager::handler(ctx)
    }

    pub fn config_remove_manager(ctx: Context<ConfigRemoveManager>) -> Result<()> {
        ConfigRemoveManager::handler(ctx)
    }

    pub fn config_claim_platform_fee(ctx: Context<ConfigClaimPlatformFee>) -> Result<()> {
        ConfigClaimPlatformFee::handler(ctx)
    }

    pub fn config_migrate(ctx: Context<ConfigMigrate>) -> Result<()> {
        ConfigMigrate::handler(ctx)
    }

    pub fn nav_override(ctx: Context<NavOverride>, total_assets: u64) -> Result<()> {
        NavOverride::handler(ctx, total_assets)
    }

    pub fn deposit_request_reject(ctx: Context<DepositRequestReject>) -> Result<()> {
        DepositRequestReject::handler(ctx)
    }

    pub fn withdrawal_request_reject(ctx: Context<WithdrawalRequestReject>) -> Result<()> {
        WithdrawalRequestReject::handler(ctx)
    }

    // Guardian

    pub fn config_pause(ctx: Context<ConfigPause>) -> Result<()> {
        ConfigPause::handler(ctx)
    }

    pub fn vault_pause(ctx: Context<VaultPause>) -> Result<()> {
        VaultPause::handler(ctx)
    }

    // Pending admin

    pub fn admin_accept(ctx: Context<AdminAccept>) -> Result<()> {
        AdminAccept::handler(ctx)
    }

    // NAV Updater

    pub fn nav_update(ctx: Context<NavUpdate>, total_assets: u64) -> Result<()> {
        NavUpdate::handler(ctx, total_assets)
    }

    // Vault Manager

    pub fn vault_initialize(
        ctx: Context<VaultInitialize>,
        args: VaultInitializeArgs,
    ) -> Result<()> {
        VaultInitialize::handler(ctx, args)
    }

    pub fn vault_update(ctx: Context<VaultUpdate>, args: VaultUpdateArgs) -> Result<()> {
        VaultUpdate::handler(ctx, args)
    }

    pub fn vault_close(ctx: Context<VaultClose>) -> Result<()> {
        VaultClose::handler(ctx)
    }

    pub fn vault_close_strategy<'info>(
        ctx: Context<'_, '_, '_, 'info, VaultCloseStrategy<'info>>,
    ) -> Result<()> {
        VaultCloseStrategy::handler(ctx)
    }

    pub fn vault_claim_manager_fee(ctx: Context<VaultClaimManagerFee>) -> Result<()> {
        VaultClaimManagerFee::handler(ctx)
    }

    // User

    pub fn deposit_request_create(ctx: Context<DepositRequestCreate>, amount: u64) -> Result<()> {
        DepositRequestCreate::handler(ctx, amount)
    }

    pub fn deposit_request_cancel(ctx: Context<DepositRequestCancel>) -> Result<()> {
        DepositRequestCancel::handler(ctx)
    }

    pub fn deposit_request_resolve(ctx: Context<DepositRequestResolve>) -> Result<()> {
        DepositRequestResolve::handler(ctx)
    }

    pub fn withdrawal_request_create(
        ctx: Context<WithdrawalRequestCreate>,
        shares: u64,
    ) -> Result<()> {
        WithdrawalRequestCreate::handler(ctx, shares)
    }

    pub fn withdrawal_request_cancel(ctx: Context<WithdrawalRequestCancel>) -> Result<()> {
        WithdrawalRequestCancel::handler(ctx)
    }

    pub fn withdrawal_request_resolve(ctx: Context<WithdrawalRequestResolve>) -> Result<()> {
        WithdrawalRequestResolve::handler(ctx)
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

    pub fn meteora_dlmm_extend_position(
        ctx: Context<MeteoraDlmmExtendPosition>,
        bins_to_add: u16,
    ) -> Result<()> {
        MeteoraDlmmExtendPosition::handler(ctx, bins_to_add)
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
        MeteoraDlmmRemoveLiquidity::handler(ctx, params, None)
    }

    pub fn meteora_dlmm_remove_liquidity_range<'info>(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmRemoveLiquidity<'info>>,
        params: MeteoraDlmmRemoveLiquidityParams,
        from_bin_id: i32,
        to_bin_id: i32,
    ) -> Result<()> {
        MeteoraDlmmRemoveLiquidity::handler(ctx, params, Some((from_bin_id, to_bin_id)))
    }

    pub fn meteora_dlmm_claim_fee<'info>(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmClaimFee<'info>>,
        remaining_accounts_info: dlmm::types::RemainingAccountsInfo,
    ) -> Result<()> {
        MeteoraDlmmClaimFee::handler(ctx, remaining_accounts_info, None)
    }

    pub fn meteora_dlmm_claim_fee_range<'info>(
        ctx: Context<'_, '_, '_, 'info, MeteoraDlmmClaimFee<'info>>,
        remaining_accounts_info: dlmm::types::RemainingAccountsInfo,
        from_bin_id: i32,
        to_bin_id: i32,
    ) -> Result<()> {
        MeteoraDlmmClaimFee::handler(ctx, remaining_accounts_info, Some((from_bin_id, to_bin_id)))
    }

    // Phoenix

    pub fn phoenix_initialize_strategy(ctx: Context<PhoenixInitializeStrategy>) -> Result<()> {
        PhoenixInitializeStrategy::handler(ctx)
    }

    pub fn phoenix_deposit_funds<'info>(
        ctx: Context<'_, '_, '_, 'info, PhoenixDepositFunds<'info>>,
        amount: u64,
    ) -> Result<()> {
        PhoenixDepositFunds::handler(ctx, amount)
    }

    pub fn phoenix_place_market_order<'info>(
        ctx: Context<'_, '_, '_, 'info, PhoenixPlaceMarketOrder<'info>>,
        params: PhoenixMarketOrderParams,
    ) -> Result<()> {
        PhoenixPlaceMarketOrder::handler(ctx, params)
    }

    pub fn phoenix_place_limit_order<'info>(
        ctx: Context<'_, '_, '_, 'info, PhoenixPlaceLimitOrder<'info>>,
        params: PhoenixLimitOrderParams,
    ) -> Result<()> {
        PhoenixPlaceLimitOrder::handler(ctx, params)
    }

    pub fn phoenix_cancel_orders<'info>(
        ctx: Context<'_, '_, '_, 'info, PhoenixCancelOrders<'info>>,
        mode: protocol::phoenix::PhoenixCancelMode,
    ) -> Result<()> {
        PhoenixCancelOrders::handler(ctx, mode)
    }

    pub fn phoenix_withdraw_funds<'info>(
        ctx: Context<'_, '_, '_, 'info, PhoenixWithdrawFunds<'info>>,
        amount: u64,
    ) -> Result<()> {
        PhoenixWithdrawFunds::handler(ctx, amount)
    }

    pub fn phoenix_ember_withdraw(ctx: Context<PhoenixEmberWithdraw>) -> Result<()> {
        PhoenixEmberWithdraw::handler(ctx)
    }
}
