use hedge_vault_litesvm::*;

#[test]
fn enforces_minimum_request_sizes() {
    let mut ctx = TestContext::new();
    let mint = ctx.create_mint(&TOKEN_PROGRAM, None);
    let mut args = TestContext::vault_args();
    args.min_deposit = 10 * USDC;
    args.min_withdrawal_shares = 50 * USDC;
    let (v, result) = ctx.initialize_vault(mint, TOKEN_PROGRAM, args);
    result.unwrap();

    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);

    assert_error(ctx.request_deposit(&v, &user, USDC), HedgeVaultError::DepositBelowMinimum);
    ctx.request_deposit(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.update_nav(&v, 0).unwrap();
    ctx.resolve_deposit_request(&v, &user.pubkey()).unwrap();

    assert_error(
        ctx.request_withdrawal(&v, &user, 40 * USDC),
        HedgeVaultError::WithdrawalBelowMinimum,
    );
    // full balance is always allowed
    ctx.request_withdrawal(&v, &user, 100 * USDC).unwrap();
}

#[test]
fn manager_updates_minimums() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();

    let mut args = TestContext::update_vault_args();
    args.min_deposit = Some(5 * USDC);
    args.min_withdrawal_shares = Some(7 * USDC);
    ctx.update_vault(&v, args).unwrap();

    let vault = ctx.vault(&v.address);
    assert_eq!(vault.min_deposit, 5 * USDC);
    assert_eq!(vault.min_withdrawal_shares, 7 * USDC);
}
