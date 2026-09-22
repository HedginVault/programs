use hedge_vault_litesvm::*;

#[test]
fn rejects_deposit_mint_with_transfer_fee() {
    let mut ctx = TestContext::new();
    let mint = ctx.create_mint(&TOKEN_2022_PROGRAM, Some(50));

    let (_, result) = ctx.vault_initialize(mint, TOKEN_2022_PROGRAM, TestContext::vault_args());

    assert_error(result, HedgeVaultError::InvalidDepositMintExtension);
}

#[test]
fn accepts_token_2022_mint_with_zero_transfer_fee() {
    let mut ctx = TestContext::new();
    let mint = ctx.create_mint(&TOKEN_2022_PROGRAM, Some(0));
    let (v, result) = ctx.vault_initialize(mint, TOKEN_2022_PROGRAM, TestContext::vault_args());
    result.unwrap();

    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();

    assert_eq!(ctx.token_balance(&deposit_escrow_pda(&v.address)), 100 * USDC);
    assert_eq!(ctx.vault(&v.address).pending_deposits, 100 * USDC);
}
