use hedge_vault_litesvm_phoenix::*;

/// 1 SOL at the fixture's 2 base-lot decimals.
const ONE_SOL: u64 = 100;
/// Rests below the fixture's best SOL bid.
const RESTING_BID_TICKS: u64 = 140_000;

fn trading() -> PhoenixVaultContext {
    let mut ctx = PhoenixVaultContext::new();
    ctx.initialize_strategy().unwrap();
    ctx.deposit(1_000 * USDC).unwrap();
    ctx
}

#[test]
fn market_order_opens_and_reduce_only_order_closes_a_position() {
    let mut ctx = trading();

    let opened = ctx
        .market_order(SOL, market_order(PhoenixSide::Bid, ONE_SOL, false))
        .unwrap();

    let placed = event::<PhoenixOrderPlaced>(&opened);
    assert_eq!(placed.base_lots_filled, ONE_SOL);
    assert!(placed.quote_lots_filled > 0);
    assert_eq!(placed.order_sequence_number, None);
    assert_eq!(ctx.trader().position_count, 1);

    let closed = ctx
        .market_order(SOL, market_order(PhoenixSide::Ask, ONE_SOL, true))
        .unwrap();

    assert_eq!(
        event::<PhoenixOrderPlaced>(&closed).base_lots_filled,
        ONE_SOL
    );
    let trader = ctx.trader();
    assert_eq!(trader.position_count, 0);
    // the round trip only costs the spread and taker fees
    let cost = (1_000 * USDC) as i64 - trader.quote_lot_collateral;
    assert!(
        cost > 0 && cost < (2 * USDC) as i64,
        "round trip cost {cost}"
    );
}

#[test]
fn limit_order_rests_until_cancelled() {
    let mut ctx = trading();

    let placed = ctx
        .limit_order(
            SOL,
            post_only_order(PhoenixSide::Bid, RESTING_BID_TICKS, 10),
        )
        .unwrap();

    let placed = event::<PhoenixOrderPlaced>(&placed);
    assert_eq!((placed.base_lots_filled, placed.base_lots_posted), (0, 10));
    assert!(placed.order_sequence_number.is_some());
    assert_eq!(ctx.trader().position_count, 1);

    ctx.cancel_orders(SOL, PhoenixCancelMode::All).unwrap();

    assert_eq!(ctx.trader().position_count, 0);
}

#[test]
fn cancel_up_to_removes_resting_orders_on_one_side() {
    let mut ctx = trading();
    ctx.limit_order(
        SOL,
        post_only_order(PhoenixSide::Bid, RESTING_BID_TICKS, 10),
    )
    .unwrap();

    ctx.cancel_orders(
        SOL,
        PhoenixCancelMode::UpTo {
            side: PhoenixSide::Bid,
            num_orders_to_cancel: None,
            tick_limit: None,
        },
    )
    .unwrap();

    assert_eq!(ctx.trader().position_count, 0);
}

#[test]
fn reduce_only_vault_allows_only_exit_paths() {
    let mut ctx = trading();
    ctx.market_order(SOL, market_order(PhoenixSide::Bid, ONE_SOL, false))
        .unwrap();
    ctx.set_vault_status(VaultStatus::ReduceOnly).unwrap();

    assert_error(
        ctx.market_order(SOL, market_order(PhoenixSide::Bid, ONE_SOL, false)),
        HedgeVaultError::VaultNotOperational,
    );
    assert_error(
        ctx.limit_order(
            SOL,
            post_only_order(PhoenixSide::Bid, RESTING_BID_TICKS, 10),
        ),
        HedgeVaultError::VaultNotOperational,
    );
    assert_error(ctx.deposit(USDC), HedgeVaultError::VaultNotOperational);

    ctx.market_order(SOL, market_order(PhoenixSide::Ask, ONE_SOL, true))
        .unwrap();
    ctx.cancel_orders(SOL, PhoenixCancelMode::All).unwrap();
    ctx.warp_slots(1_000);
    ctx.withdraw(100 * USDC).unwrap();
}

#[test]
fn reduce_only_protocol_allows_only_exit_paths() {
    let mut ctx = trading();
    ctx.market_order(SOL, market_order(PhoenixSide::Bid, ONE_SOL, false))
        .unwrap();
    ctx.set_protocol_status(ProtocolStatus::ReduceOnly).unwrap();

    assert_error(
        ctx.market_order(SOL, market_order(PhoenixSide::Bid, ONE_SOL, false)),
        HedgeVaultError::ProtocolNotOperational,
    );

    ctx.market_order(SOL, market_order(PhoenixSide::Ask, ONE_SOL, true))
        .unwrap();
    ctx.warp_slots(1_000);
    ctx.withdraw(100 * USDC).unwrap();
}
