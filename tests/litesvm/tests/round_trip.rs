use hedge_vault_litesvm::*;

#[test]
fn deposit_and_withdraw_round_trip() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ata(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    assert_eq!(ctx.token_balance(&user_shares), 100 * USDC);
    assert!(ctx.deposit_request(&deposit_request_pda(&v.address, &user.pubkey())).is_none());

    ctx.withdrawal_request_create(&v, &user, 40 * USDC).unwrap();
    ctx.warp_days(1);
    // 10% gain: 110 USDC backing 100 shares
    ctx.nav_update(&v, 110 * USDC).unwrap();
    ctx.withdrawal_request_resolve(&v, &user.pubkey()).unwrap();

    assert_eq!(ctx.token_balance(&user_shares), 60 * USDC);
    assert_eq!(ctx.token_balance(&user_assets), 944 * USDC);
    assert_eq!(ctx.vault(&v.address).pending_withdrawal_shares, 0);
}
