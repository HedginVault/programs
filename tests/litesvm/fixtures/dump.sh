#!/usr/bin/env sh
# Dumps the mainnet programs that tests/jupiter_routes.rs routes through.
set -e
cd "$(dirname "$0")"
solana program dump -u m JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 jupiter.so
solana program dump -u m SwaPpA9LAaLfeLi3a68M4DjnLqgtticKg6CnyNwgAC8 spl_token_swap.so
