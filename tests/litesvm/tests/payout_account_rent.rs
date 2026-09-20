//! A request carries the rent for the payout token account it will need, so settlement can create
//! that account itself. Before this, the account was created with the request and its owner could
//! close it to reclaim the rent, leaving the request unresolvable by anyone.

use hedge_vault_litesvm::*;

/// Rent of a request account: what the depositor gets back when it closes, on top of any escrow the
/// settlement did not spend.
fn request_rent(ctx: &TestContext, request: &Pubkey) -> u64 {
    ctx.lamports(request) - ctx.deposit_request(request).map_or(0, |r| r.rent_escrow)
}

#[test]
fn resolves_when_the_depositor_has_no_share_account() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ata(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    assert!(!ctx.account_exists(&user_shares), "create must not open the share account");

    let request = deposit_request_pda(&v.address, &user.pubkey());
    let escrow = ctx.payout_escrow(&v);
    assert_eq!(ctx.deposit_request(&request).unwrap().rent_escrow, escrow);
    assert_eq!(ctx.lamports(&request), request_rent(&ctx, &request) + escrow);

    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();

    // a third party settles, and pays nothing for the account it has to create
    let resolver = ctx.new_user();
    let resolver_before = ctx.lamports(&resolver.pubkey());
    ctx.deposit_request_resolve_by(&v, &user.pubkey(), Some(&resolver)).unwrap();

    assert_eq!(ctx.lamports(&resolver.pubkey()), resolver_before);
    assert_eq!(ctx.token_balance(&user_shares), 100 * USDC);
    assert_eq!(ctx.lamports(&user_shares), ctx.token_account_rent(&v.share_mint));
    assert!(ctx.deposit_request(&request).is_none());
}

#[test]
fn resolves_after_the_depositor_closes_their_share_account() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ata(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);

    // a first round trip leaves the share account open but empty
    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();
    ctx.withdrawal_request_create(&v, &user, 100 * USDC).unwrap();
    assert_eq!(ctx.token_balance(&user_shares), 0);

    // the depositor reclaims its rent, as a wallet's "close empty accounts" does
    ctx.close_token_account(&user, &user_shares, &TOKEN_PROGRAM).unwrap();
    assert!(!ctx.account_exists(&user_shares));

    ctx.deposit_request_create(&v, &user, 50 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 100 * USDC).unwrap();

    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    assert_eq!(ctx.token_balance(&user_shares), 50 * USDC);
}

#[test]
fn refunds_the_whole_escrow_when_the_share_account_already_exists() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ctx.create_token_account(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    let request = deposit_request_pda(&v.address, &user.pubkey());
    let refund = ctx.lamports(&request);

    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();

    let user_before = ctx.lamports(&user.pubkey());
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    // nothing was spent on the account, so rent and escrow both go back to the depositor
    assert_eq!(ctx.lamports(&user.pubkey()), user_before + refund);
    assert_eq!(ctx.token_balance(&user_shares), 100 * USDC);
}

#[test]
fn cancel_refunds_the_escrow() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);

    let before = ctx.lamports(&user.pubkey());
    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    assert_eq!(ctx.lamports(&user.pubkey()), before - ctx.lamports(&deposit_request_pda(&v.address, &user.pubkey())));

    ctx.deposit_request_cancel(&v, &user).unwrap();

    assert_eq!(ctx.lamports(&user.pubkey()), before);
}

#[test]
fn reject_recreates_a_closed_deposit_account() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 100 * USDC);

    // depositing the whole balance empties the account, so the depositor can close it
    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.close_token_account(&user, &user_assets, &v.deposit_token_program).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();

    ctx.deposit_request_reject(&v, &user.pubkey(), None).unwrap();

    assert_eq!(ctx.token_balance(&user_assets), 100 * USDC);
    assert_eq!(ctx.token_balance(&deposit_escrow_pda(&v.address)), 0);
}

#[test]
fn cancel_recreates_a_closed_deposit_account() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 100 * USDC);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.close_token_account(&user, &user_assets, &v.deposit_token_program).unwrap();

    ctx.deposit_request_cancel(&v, &user).unwrap();

    assert_eq!(ctx.token_balance(&user_assets), 100 * USDC);
}

#[test]
fn withdrawal_resolve_recreates_a_closed_payout_account() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 100 * USDC);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    // the deposit account is empty now, so it can be closed while the withdrawal is pending
    ctx.withdrawal_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.close_token_account(&user, &user_assets, &v.deposit_token_program).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 100 * USDC).unwrap();

    let resolver = ctx.new_user();
    let resolver_before = ctx.lamports(&resolver.pubkey());
    ctx.withdrawal_request_resolve_by(&v, &user.pubkey(), Some(&resolver)).unwrap();

    assert_eq!(ctx.lamports(&resolver.pubkey()), resolver_before);
    assert_eq!(ctx.token_balance(&user_assets), 100 * USDC);
}

#[test]
fn withdrawal_reject_recreates_a_closed_share_account() {
    let mut ctx = TestContext::new();
    let v = ctx.setup_vault();
    let user = ctx.new_user();
    ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 1_000 * USDC);
    let user_shares = ata(&user.pubkey(), &v.share_mint, &TOKEN_PROGRAM);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    // every share sits in the escrow, so the share account is empty and closeable
    ctx.withdrawal_request_create(&v, &user, 100 * USDC).unwrap();
    ctx.close_token_account(&user, &user_shares, &TOKEN_PROGRAM).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 100 * USDC).unwrap();

    ctx.withdrawal_request_reject(&v, &user.pubkey(), None).unwrap();

    assert_eq!(ctx.token_balance(&user_shares), 100 * USDC);
    assert_eq!(ctx.token_balance(&share_escrow_pda(&v.address)), 0);
}

#[test]
fn escrow_covers_a_token_2022_deposit_mint() {
    let mut ctx = TestContext::new();
    let mint = ctx.create_mint(&TOKEN_2022_PROGRAM, Some(0));
    let (v, result) = ctx.vault_initialize(mint, TOKEN_2022_PROGRAM, TestContext::vault_args());
    result.unwrap();

    let user = ctx.new_user();
    let user_assets = ctx.fund(&user.pubkey(), &v.deposit_mint, &v.deposit_token_program, 100 * USDC);

    ctx.deposit_request_create(&v, &user, 100 * USDC).unwrap();

    // a Token-2022 payout account is larger than the share account, and sets the escrow
    let deposit_rent = ctx.token_account_rent(&v.deposit_mint);
    assert!(deposit_rent > ctx.token_account_rent(&v.share_mint));
    let request = withdrawal_request_pda(&v.address, &user.pubkey());
    assert_eq!(
        ctx.deposit_request(&deposit_request_pda(&v.address, &user.pubkey())).unwrap().rent_escrow,
        deposit_rent
    );

    ctx.warp_days(1);
    ctx.nav_update(&v, 0).unwrap();
    ctx.deposit_request_resolve(&v, &user.pubkey()).unwrap();

    ctx.withdrawal_request_create(&v, &user, 100 * USDC).unwrap();
    assert_eq!(ctx.withdrawal_request(&request).unwrap().rent_escrow, deposit_rent);
    ctx.close_token_account(&user, &user_assets, &v.deposit_token_program).unwrap();
    ctx.warp_days(1);
    ctx.nav_update(&v, 100 * USDC).unwrap();

    let resolver = ctx.new_user();
    let resolver_before = ctx.lamports(&resolver.pubkey());
    ctx.withdrawal_request_resolve_by(&v, &user.pubkey(), Some(&resolver)).unwrap();

    assert_eq!(ctx.lamports(&resolver.pubkey()), resolver_before);
    assert_eq!(ctx.token_balance(&user_assets), 100 * USDC);
    assert_eq!(ctx.lamports(&user_assets), deposit_rent);
}
