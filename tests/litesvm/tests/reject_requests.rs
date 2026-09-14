use hedge_vault_litesvm::*;

#[test]
fn admin_rejects_pending_deposit() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();

    let stranger = ctx.new_user();
    assert_error(
        ctx.deposit_request_reject(&v, &user.pubkey(), Some(&stranger)),
        HedgeVaultError::InvalidAdmin,
    );

    ctx.deposit_request_reject(&v, &user.pubkey(), None).unwrap();

    assert_eq!(ctx.token_balance(&user_assets), 1_000 * USDC);
    assert_eq!(ctx.token_balance(&deposit_escrow_pda(&v.address)), 0);
    assert_eq!(ctx.vault(&v.address).pending_deposits, 0);
    assert!(ctx.deposit_request(&deposit_request_pda(&v.address, &user.pubkey())).is_none());
}

#[test]
fn admin_rejects_withdrawal_after_its_nav_is_posted() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ata(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    ctx.withdrawal_request_create(&v, &user, 40 * USDC).unwrap();
    ctx.warp_days(1);
    // request is now resolvable and no longer cancellable by the user
    ctx.nav_update(&v, 100 * USDC).unwrap();

    ctx.withdrawal_request_reject(&v, &user.pubkey(), None).unwrap();

    assert_eq!(ctx.token_balance(&user_shares), 100 * USDC);
    assert_eq!(ctx.token_balance(&share_escrow_pda(&v.address)), 0);
    assert_eq!(ctx.vault(&v.address).pending_withdrawal_shares, 0);
    assert!(ctx
        .withdrawal_request(&withdrawal_request_pda(&v.address, &user.pubkey()))
        .is_none());
}
