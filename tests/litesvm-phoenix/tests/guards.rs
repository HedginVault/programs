use hedge_vault_litesvm_phoenix::*;

fn trading() -> PhoenixVaultContext {
    let mut ctx = PhoenixVaultContext::new();
    ctx.initialize_strategy().unwrap();
    ctx.deposit(1_000 * USDC).unwrap();
    ctx
}

fn replace(instruction: &mut solana_instruction::Instruction, from: Pubkey, to: Pubkey) {
    let meta = instruction
        .accounts
        .iter_mut()
        .find(|meta| meta.pubkey == from)
        .unwrap();
    meta.pubkey = to;
}

#[test]
fn rejects_a_global_vault_the_exchange_does_not_name() {
    let mut ctx = trading();
    let mut deposit = ctx.deposit_ix(USDC);
    replace(
        &mut deposit,
        ctx.exchange.global_vault,
        ctx.exchange.perp_asset_map,
    );

    assert_error(ctx.send(&[deposit]), HedgeVaultError::InvalidPhoenixAccount);
}

#[test]
fn rejects_an_incomplete_trader_index_tail() {
    let mut ctx = trading();
    let mut deposit = ctx.deposit_ix(USDC);
    deposit.accounts.pop();

    assert_error(
        ctx.send(&[deposit]),
        HedgeVaultError::InvalidPhoenixRemainingAccounts,
    );
}

#[test]
fn rejects_a_spline_of_another_market() {
    let mut ctx = trading();
    let mut order = ctx.market_order_ix(SOL, market_order(PhoenixSide::Bid, 10, false));
    replace(
        &mut order,
        ctx.market(SOL).spline_collection,
        ctx.market("BTC").spline_collection,
    );

    assert_error(ctx.send(&[order]), HedgeVaultError::InvalidPhoenixAccount);
}

#[test]
fn rejects_a_withdraw_queue_the_exchange_does_not_name() {
    let mut ctx = trading();
    let mut withdraw = ctx.withdraw_ix(USDC);
    replace(
        &mut withdraw,
        ctx.exchange.withdraw_queue,
        ctx.exchange.global_vault,
    );

    assert_error(
        ctx.send(&[withdraw]),
        HedgeVaultError::InvalidPhoenixAccount,
    );
}
