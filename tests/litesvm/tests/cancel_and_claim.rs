use hedge_vault_litesvm::*;

/// Deposits `amount` for `user` and resolves it at NAV 1.
fn deposit_and_resolve(ctx: &mut TestContext, v: &TestVault, user: &Keypair, amount: u64) {
    ctx.request_deposit(v, user, amount).unwrap();
    ctx.warp_days(1);
    ctx.update_nav(v, 0).unwrap();
    ctx.resolve_deposit_request(v, &user.pubkey()).unwrap();
}

#[test]
fn depositor_cancels_pending_deposit() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);

    ctx.request_deposit(&v, &user, 100 * USDC).unwrap();
    ctx.cancel_deposit_request(&v, &user).unwrap();

    assert_eq!(ctx.token_balance(&user_assets), 1_000 * USDC);
    assert_eq!(ctx.token_balance(&deposit_escrow_pda(&v.address)), 0);
    assert_eq!(ctx.vault(&v.address).pending_deposits, 0);
    assert!(ctx.deposit_request(&deposit_request_pda(&v.address, &user.pubkey())).is_none());
}

#[test]
fn withdrawer_cancels_pending_withdrawal() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ata(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);
    deposit_and_resolve(&mut ctx, &v, &user, 100 * USDC);

    ctx.request_withdrawal(&v, &user, 40 * USDC).unwrap();
    ctx.cancel_withdrawal_request(&v, &user).unwrap();

    assert_eq!(ctx.token_balance(&user_shares), 100 * USDC);
    assert_eq!(ctx.token_balance(&share_escrow_pda(&v.address)), 0);
    assert_eq!(ctx.vault(&v.address).pending_withdrawal_shares, 0);
    assert!(ctx
        .withdrawal_request(&withdrawal_request_pda(&v.address, &user.pubkey()))
        .is_none());
}

/// Vault with a 20% manager and 10% platform performance fee that has accrued fees on a 10% gain.
fn vault_with_accrued_fees(ctx: &mut TestContext) -> TestVault {
    let mut config_args = TestContext::update_config_args();
    config_args.platform_performance_fee_bps = Some(1_000);
    ctx.update_config(config_args).unwrap();

    let mint = ctx.create_mint(&TOKEN_PROGRAM, None);
    let mut vault_args = TestContext::vault_args();
    vault_args.performance_fee_bps = 2_000;
    let (v, result) = ctx.initialize_vault(mint, TOKEN_PROGRAM, vault_args);
    result.unwrap();

    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    deposit_and_resolve(ctx, &v, &user, 100 * USDC);

    ctx.warp_days(1);
    ctx.update_nav(&v, 110 * USDC).unwrap();

    v
}

#[test]
fn manager_claims_fee_shares() {
    let mut ctx = TestContext::new();
    let v = vault_with_accrued_fees(&mut ctx);
    let owed = ctx.vault(&v.address).unclaimed_manager_fee_shares;
    assert!(owed > 0);

    ctx.claim_manager_fee(&v).unwrap();

    let admin_shares = ata(&ctx.admin.pubkey(), &v.share_mint, &TOKEN_PROGRAM);
    assert_eq!(ctx.token_balance(&admin_shares), owed);
    assert_eq!(ctx.vault(&v.address).unclaimed_manager_fee_shares, 0);
}

#[test]
fn treasury_claims_platform_fee_shares() {
    let mut ctx = TestContext::new();
    let v = vault_with_accrued_fees(&mut ctx);
    let owed = ctx.vault(&v.address).unclaimed_platform_fee_shares;
    assert!(owed > 0);

    ctx.claim_platform_fee(&v).unwrap();

    let treasury_shares = ata(&ctx.admin.pubkey(), &v.share_mint, &TOKEN_PROGRAM);
    assert_eq!(ctx.token_balance(&treasury_shares), owed);
    assert_eq!(ctx.vault(&v.address).unclaimed_platform_fee_shares, 0);
}
