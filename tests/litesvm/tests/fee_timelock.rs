use hedge_vault_litesvm::*;

#[test]
fn manager_fee_increase_waits_for_the_delay() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();

    let mut args = TestContext::update_vault_args();
    args.performance_fee_bps = Some(2_000);
    ctx.vault_update(&v, args).unwrap();

    let vault = ctx.vault(&v.address);
    assert_eq!(vault.performance_fee_bps, 0);
    assert_eq!(vault.pending_performance_fee_bps, 2_000);
    assert_eq!(vault.fee_effective_ts, START_TS + hedge_vault::FEE_INCREASE_DELAY);

    ctx.warp_days(7);
    ctx.nav_update(&v, 0).unwrap();

    let vault = ctx.vault(&v.address);
    assert_eq!(vault.performance_fee_bps, 2_000);
    assert_eq!(vault.fee_effective_ts, 0);
}
