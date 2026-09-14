use hedge_vault_litesvm::*;

#[test]
fn enforces_minimum_request_sizes() {
    let mut ctx = TestContext::new();
    let mint = ctx.create_mint(&TOKEN_PROGRAM, None);
    let mut args = TestContext::vault_args();
    args.min_deposit = 10 * USDC;
    args.min_withdrawal_shares = 50 * USDC;
    let (v, result) = ctx.vault_initialize(mint, TOKEN_PROGRAM, args);
    result.unwrap();

    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);

    assert_error(ctx.deposit_request_create(&v, &user, USDC), HedgeVaultError::DepositBelowMinimum);
    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    assert_error(
        ctx.withdrawal_request_create(&v, &user, 40 * USDC),
        HedgeVaultError::WithdrawalBelowMinimum,
    );
    // full balance is always allowed
    ctx.withdrawal_request_create(&v, &user, 100 * USDC).unwrap();
}

#[test]
fn initialize_vault_rejects_a_zero_minimum() {
    let mut ctx = TestContext::new();
    let mint = ctx.create_mint(&TOKEN_PROGRAM, None);

    let mut args = TestContext::vault_args();
    args.min_deposit = 0;
    let (_, result) = ctx.vault_initialize(mint, TOKEN_PROGRAM, args);
    assert_error(result, HedgeVaultError::InvalidMinimumAmount);

    let mut args = TestContext::vault_args();
    args.min_withdrawal_shares = 0;
    let (_, result) = ctx.vault_initialize(mint, TOKEN_PROGRAM, args);
    assert_error(result, HedgeVaultError::InvalidMinimumAmount);
}

#[test]
fn update_vault_rejects_a_zero_minimum() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();

    let mut args = TestContext::update_vault_args();
    args.min_deposit = Some(0);
    assert_error(ctx.vault_update(&v, args), HedgeVaultError::InvalidMinimumAmount);

    let mut args = TestContext::update_vault_args();
    args.min_withdrawal_shares = Some(0);
    assert_error(ctx.vault_update(&v, args), HedgeVaultError::InvalidMinimumAmount);
}

#[test]
fn manager_updates_minimums() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();

    let mut args = TestContext::update_vault_args();
    args.min_deposit = Some(5 * USDC);
    args.min_withdrawal_shares = Some(7 * USDC);
    ctx.vault_update(&v, args).unwrap();

    let vault = ctx.vault(&v.address);
    assert_eq!(vault.min_deposit, 5 * USDC);
    assert_eq!(vault.min_withdrawal_shares, 7 * USDC);
}
