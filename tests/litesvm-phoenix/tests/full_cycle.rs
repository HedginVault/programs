use hedge_vault_litesvm_phoenix::*;

/// initialize → deposit → open → reduce-only close → withdraw everything → close strategy.
#[test]
fn full_cycle_returns_the_collateral_minus_trading_costs() {
    let mut ctx = PhoenixVaultContext::new();

    ctx.initialize_strategy().unwrap();
    ctx.deposit(1_000 * USDC).unwrap();
    ctx.market_order(SOL, market_order(PhoenixSide::Bid, 100, false))
        .unwrap();
    ctx.market_order(SOL, market_order(PhoenixSide::Ask, 100, true))
        .unwrap();

    ctx.warp_slots(1_000);
    let collateral = ctx.trader().quote_lot_collateral as u64;
    let withdrawn = ctx.withdraw(collateral).unwrap();
    assert_eq!(
        event::<PhoenixFundsWithdrawn>(&withdrawn).received,
        collateral
    );

    ctx.close_strategy().unwrap();

    assert_eq!(ctx.vault_usdc(), VAULT_USDC - 1_000 * USDC + collateral);
    assert_eq!(ctx.open_strategy_count(), 0);
}
