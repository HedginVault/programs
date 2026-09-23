use hedge_vault_litesvm_phoenix::*;

fn funded() -> PhoenixVaultContext {
    let mut ctx = PhoenixVaultContext::new();
    ctx.initialize_strategy().unwrap();
    ctx
}

#[test]
fn deposit_requires_an_onboarded_trader() {
    let mut ctx = funded();
    ctx.set_trader_flags(0);

    assert_error(
        ctx.deposit(100 * USDC),
        HedgeVaultError::PhoenixTraderNotReady,
    );
}

#[test]
fn deposit_moves_idle_usdc_into_trader_collateral() {
    let mut ctx = funded();

    let deposited = ctx.deposit(1_000 * USDC).unwrap();

    assert_eq!(
        event::<PhoenixFundsDeposited>(&deposited).amount,
        1_000 * USDC
    );
    assert_eq!(ctx.vault_usdc(), VAULT_USDC - 1_000 * USDC);
    assert_eq!(ctx.token_balance(&ctx.vault_canonical_token_account()), 0);
    // one quote lot is one canonical token atom
    assert_eq!(ctx.trader().quote_lot_collateral, (1_000 * USDC) as i64);
}

#[test]
fn withdraw_within_the_budget_returns_usdc_immediately() {
    let mut ctx = funded();
    ctx.deposit(1_000 * USDC).unwrap();
    ctx.warp_slots(1_000);

    let withdrawn = ctx.withdraw(400 * USDC).unwrap();

    let withdrawal = event::<PhoenixFundsWithdrawn>(&withdrawn);
    assert_eq!(
        (withdrawal.requested, withdrawal.received, withdrawal.queued),
        (400 * USDC, 400 * USDC, false)
    );
    assert_eq!(ctx.vault_usdc(), VAULT_USDC - 600 * USDC);
    assert_eq!(ctx.token_balance(&ctx.vault_canonical_token_account()), 0);
    assert_eq!(ctx.trader().quote_lot_collateral, (600 * USDC) as i64);
}

#[test]
fn queued_withdrawal_is_unwrapped_once_delivered() {
    let mut ctx = funded();
    ctx.deposit(1_000 * USDC).unwrap();
    ctx.warp_slots(1_000);
    ctx.set_withdraw_budget(100 * USDC, 1);
    ctx.withdraw(90 * USDC).unwrap();

    let queued = ctx.withdraw(50 * USDC).unwrap();

    let withdrawal = event::<PhoenixFundsWithdrawn>(&queued);
    assert_eq!((withdrawal.received, withdrawal.queued), (0, true));
    let trader = ctx.trader();
    assert_ne!(trader.withdraw_queue_node, 0);
    // Phoenix keeps queued funds in collateral until the crank pays them out
    assert_eq!(trader.quote_lot_collateral, (910 * USDC) as i64);
    assert_error(
        ctx.close_strategy(),
        HedgeVaultError::PhoenixStrategyNotEmpty,
    );

    ctx.deliver_canonical(50 * USDC);
    let unwrapped = ctx.ember_withdraw().unwrap();

    assert_eq!(
        event::<PhoenixCanonicalUnwrapped>(&unwrapped).amount,
        50 * USDC
    );
    assert_eq!(ctx.token_balance(&ctx.vault_canonical_token_account()), 0);
    assert_eq!(ctx.vault_usdc(), VAULT_USDC - 1_000 * USDC + 140 * USDC);
}

#[test]
fn ember_withdraw_rejects_an_empty_canonical_balance() {
    let mut ctx = funded();

    assert_error(ctx.ember_withdraw(), HedgeVaultError::InvalidTransferAmount);
}
