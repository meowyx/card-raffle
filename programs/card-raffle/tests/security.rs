//! Negative tests. Each asserts the specific Anchor error code.

mod common;

use anchor_lang::{system_program, InstructionData, ToAccountMetas};
use anchor_spl;
use card_raffle::{accounts as cr_accts, instruction as cr_ix, ID};
use common::*;
use litesvm::{types::TransactionResult, LiteSVM};
use solana_sdk::{
    instruction::{Instruction, InstructionError},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::TransactionError,
};

const ANCHOR_ERR_OFFSET: u32 = 6000;

// RaffleError variant indexes (declaration order).
const ERR_NOT_BACKEND_SIGNER: u32 = 1;
const ERR_DEADLINE_NOT_REACHED: u32 = 14;
const ERR_DROP_NOT_CLOSED: u32 = 17;
const ERR_INVALID_RANDOMNESS_ACCOUNT: u32 = 21;
const ERR_DROP_NOT_IN_RANDOMNESS_REQUESTED: u32 = 25;
const ERR_WRONG_RANDOMNESS_OWNER: u32 = 26;
const ERR_RANDOMNESS_EXPIRED: u32 = 27;
const ERR_RANDOMNESS_ALREADY_REVEALED: u32 = 28;
const ERR_RANDOMNESS_NOT_RESOLVED: u32 = 29;
const ERR_EXCEEDS_EPOCH_CREDIT: u32 = 30;

fn assert_anchor_err(result: TransactionResult, expected_variant: u32) {
    let failed = result.expect_err("tx should have failed");
    match failed.err {
        TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
            assert_eq!(
                code,
                ANCHOR_ERR_OFFSET + expected_variant,
                "expected anchor error {}, got {} (logs: {:?})",
                ANCHOR_ERR_OFFSET + expected_variant,
                code,
                failed.meta.logs
            );
        }
        other => panic!(
            "expected Custom error, got {:?} (logs: {:?})",
            other, failed.meta.logs
        ),
    }
}

#[test]
fn replay_credit_tickets_fails() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    let user = Keypair::new();
    for kp in [&admin, &backend, &user] {
        fund(&mut svm, kp, 5);
    }

    init_config(&mut svm, &admin, &backend);
    init_user(&mut svm, &user);

    credit(&mut svm, &backend, &user, "payment-1", 10).expect("first credit ok");

    let result = credit(&mut svm, &backend, &user, "payment-1", 10);
    let failed = result.expect_err("replay should fail");

    // System Program rejects the duplicate PDA allocation before any
    // program logic runs; surfaces as a non-Custom InstructionError.
    match failed.err {
        TransactionError::InstructionError(_, _) => {}
        other => panic!("expected InstructionError, got {:?}", other),
    }
}

#[test]
fn credit_from_wrong_signer_fails() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    let outsider = Keypair::new();
    let user = Keypair::new();
    for kp in [&admin, &backend, &outsider, &user] {
        fund(&mut svm, kp, 5);
    }

    init_config(&mut svm, &admin, &backend);
    init_user(&mut svm, &user);

    let result = credit(&mut svm, &outsider, &user, "payment-evil", 5);
    assert_anchor_err(result, ERR_NOT_BACKEND_SIGNER);
}

#[test]
fn close_before_deadline_fails() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    for kp in [&admin, &backend] {
        fund(&mut svm, kp, 5);
    }

    init_config(&mut svm, &admin, &backend);

    let drop_id = 1u64;
    let drop_key = drop_pda(drop_id);
    let deadline = now_ts(&svm) + 60;
    init_drop(&mut svm, &admin, drop_id, deadline);

    let result = close(&mut svm, &admin, drop_key);
    assert_anchor_err(result, ERR_DEADLINE_NOT_REACHED);
}

#[test]
fn request_randomness_with_stale_seed_fails() {
    let mut svm = setup_svm();
    let (drop_key, _user, _mint) = setup_closed_drop(&mut svm);

    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    // Fresh requires seed_slot == 99; 50 is stale.
    write_randomness(&mut svm, randomness_key, 50, 0, [0u8; 32]);

    let result = call_request_randomness(&mut svm, drop_key, randomness_key);
    assert_anchor_err(result, ERR_RANDOMNESS_EXPIRED);
}

#[test]
fn request_randomness_with_already_revealed_fails() {
    let mut svm = setup_svm();
    let (drop_key, _user, _mint) = setup_closed_drop(&mut svm);

    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    // reveal_slot == current slot → get_value succeeds → already revealed.
    write_randomness(&mut svm, randomness_key, 99, 100, [0u8; 32]);

    let result = call_request_randomness(&mut svm, drop_key, randomness_key);
    assert_anchor_err(result, ERR_RANDOMNESS_ALREADY_REVEALED);
}

#[test]
fn request_randomness_wrong_owner_fails() {
    let mut svm = setup_svm();
    let (drop_key, _user, _mint) = setup_closed_drop(&mut svm);

    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    // Data shape is valid but owner is System Program, not Switchboard.
    let imposter_owner = system_program::ID;
    write_randomness_with_owner(&mut svm, randomness_key, imposter_owner, 99, 0, [0u8; 32]);

    let result = call_request_randomness(&mut svm, drop_key, randomness_key);
    assert_anchor_err(result, ERR_WRONG_RANDOMNESS_OWNER);
}

#[test]
fn request_randomness_before_close_fails() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    for kp in [&admin, &backend] {
        fund(&mut svm, kp, 5);
    }
    init_config(&mut svm, &admin, &backend);

    let drop_id = 1u64;
    let drop_key = drop_pda(drop_id);
    let deadline = now_ts(&svm) + 60;
    init_drop(&mut svm, &admin, drop_id, deadline);

    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    write_randomness(&mut svm, randomness_key, 99, 0, [0u8; 32]);

    let result = call_request_randomness(&mut svm, drop_key, randomness_key);
    assert_anchor_err(result, ERR_DROP_NOT_CLOSED);
}

#[test]
fn settle_with_wrong_randomness_account_fails() {
    let mut svm = setup_svm();
    let (drop_key, user, mint) = setup_closed_drop_with_entry(&mut svm);

    let bound_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    write_randomness(&mut svm, bound_key, 99, 0, [0u8; 32]);
    call_request_randomness(&mut svm, drop_key, bound_key).expect("request ok");

    let mut value = [0u8; 32];
    value[0..8].copy_from_slice(&2u64.to_le_bytes());
    write_randomness(&mut svm, bound_key, 99, 200, value);

    // Attempt to settle with a different randomness account than the bound one.
    let imposter_key = Pubkey::new_unique();
    write_randomness(&mut svm, imposter_key, 99, 200, value);
    warp_slot(&mut svm, 200);

    let result = call_settle(&mut svm, drop_key, imposter_key, &user, mint);
    assert_anchor_err(result, ERR_INVALID_RANDOMNESS_ACCOUNT);
}

#[test]
fn settle_with_unrevealed_randomness_fails() {
    let mut svm = setup_svm();
    let (drop_key, user, mint) = setup_closed_drop_with_entry(&mut svm);

    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    write_randomness(&mut svm, randomness_key, 99, 0, [0u8; 32]);
    call_request_randomness(&mut svm, drop_key, randomness_key).expect("request ok");

    // Settle without updating reveal_slot — get_value fails.
    let result = call_settle(&mut svm, drop_key, randomness_key, &user, mint);
    assert_anchor_err(result, ERR_RANDOMNESS_NOT_RESOLVED);
}

#[test]
fn double_settle_fails() {
    let mut svm = setup_svm();
    let (drop_key, user, mint) = setup_closed_drop_with_entry(&mut svm);

    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    write_randomness(&mut svm, randomness_key, 99, 0, [0u8; 32]);
    call_request_randomness(&mut svm, drop_key, randomness_key).expect("request ok");

    let mut value = [0u8; 32];
    value[0..8].copy_from_slice(&2u64.to_le_bytes());
    write_randomness(&mut svm, randomness_key, 99, 200, value);
    warp_slot(&mut svm, 200);

    call_settle(&mut svm, drop_key, randomness_key, &user, mint).expect("first settle");

    // Drop is now Settled; state check rejects re-entry.
    let result = call_settle(&mut svm, drop_key, randomness_key, &user, mint);
    assert_anchor_err(result, ERR_DROP_NOT_IN_RANDOMNESS_REQUESTED);
}

#[test]
fn credit_exceeds_epoch_cap_fails() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    let user = Keypair::new();
    for kp in [&admin, &backend, &user] {
        fund(&mut svm, kp, 5);
    }
    // 2 × 100 > 150 epoch cap.
    init_config_with_caps(&mut svm, &admin, &backend, 100, 150, 1_000_000);
    init_user(&mut svm, &user);

    credit(&mut svm, &backend, &user, "epoch-1", 100).expect("first credit ok");

    let result = credit(&mut svm, &backend, &user, "epoch-2", 100);
    assert_anchor_err(result, ERR_EXCEEDS_EPOCH_CREDIT);
}

#[test]
fn credit_rolls_over_after_epoch() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    let user = Keypair::new();
    for kp in [&admin, &backend, &user] {
        fund(&mut svm, kp, 5);
    }
    init_config_with_caps(&mut svm, &admin, &backend, 100, 150, 1_000_000);
    init_user(&mut svm, &user);

    // Start at a known timestamp — LiteSVM defaults to 0, which collides
    // with the "first credit always resets" edge case.
    warp_clock(&mut svm, 1_700_000_000);
    credit(&mut svm, &backend, &user, "rollover-1", 100).expect("first credit ok");

    let blocked = credit(&mut svm, &backend, &user, "rollover-2", 100);
    assert_anchor_err(blocked, ERR_EXCEEDS_EPOCH_CREDIT);

    warp_clock(&mut svm, 1_700_000_000 + 25 * 3600);
    credit(&mut svm, &backend, &user, "rollover-3", 100).expect("post-rollover credit ok");
}

// ─── Setup helpers ────────────────────────────────────────────

fn setup_closed_drop(svm: &mut LiteSVM) -> (Pubkey, Keypair, Pubkey) {
    let admin = Keypair::new();
    let backend = Keypair::new();
    for kp in [&admin, &backend] {
        fund(svm, kp, 5);
    }
    let mint = init_config(svm, &admin, &backend);

    let drop_id = 1u64;
    let drop_key = drop_pda(drop_id);
    let deadline = now_ts(svm) + 60;
    init_drop(svm, &admin, drop_id, deadline);

    // At least one entry is needed — NoEntries fires before the randomness checks.
    let user = Keypair::new();
    fund(svm, &user, 5);
    init_user(svm, &user);
    credit(svm, &backend, &user, "p1", 10).expect("credit");
    enter(svm, &user, drop_key, 5);

    warp_clock(svm, deadline + 1);
    close(svm, &admin, drop_key).expect("close");

    // Winner ATA must exist for settle_drop's account context.
    let _ = create_user_ata(svm, &admin, &user.pubkey(), &mint);

    (drop_key, user, mint)
}

fn setup_closed_drop_with_entry(svm: &mut LiteSVM) -> (Pubkey, Keypair, Pubkey) {
    setup_closed_drop(svm)
}

// ─── Instruction helpers ──────────────────────────────────────

fn init_config(svm: &mut LiteSVM, admin: &Keypair, backend: &Keypair) -> Pubkey {
    init_config_with_caps(svm, admin, backend, 1_000_000, 50_000_000, 1_000_000)
}

/// Returns the mint pubkey so callers can build token accounts.
fn init_config_with_caps(
    svm: &mut LiteSVM,
    admin: &Keypair,
    backend: &Keypair,
    max_credit_per_call: u64,
    max_credit_per_epoch: u64,
    ticket_price: u64,
) -> Pubkey {
    let mint = create_test_mint(svm, admin, &admin.pubkey());
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeConfig {
                admin: admin.pubkey(),
                config: config_pda(),
                token_mint: mint,
                treasury_ata: treasury_ata(&mint),
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::InitializeConfig {
                backend_signer: backend.pubkey(),
                max_credit_per_call,
                max_credit_per_epoch,
                ticket_price,
            }
            .data(),
        },
        admin,
        &[admin],
    )
    .expect("initialize_config");
    mint
}

fn init_user(svm: &mut LiteSVM, user: &Keypair) {
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeUser {
                user_signer: user.pubkey(),
                user_account: user_pda(&user.pubkey()),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::InitializeUser {}.data(),
        },
        user,
        &[user],
    )
    .expect("initialize_user");
}

fn credit(
    svm: &mut LiteSVM,
    backend_or_imposter: &Keypair,
    user: &Keypair,
    payment_label: &str,
    amount: u64,
) -> TransactionResult {
    let pid = make_payment_id(payment_label);
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::CreditTickets {
                config: config_pda(),
                backend_signer: backend_or_imposter.pubkey(),
                user_account: user_pda(&user.pubkey()),
                payment_receipt: receipt_pda(&pid),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::CreditTickets {
                payment_id: pid,
                amount,
            }
            .data(),
        },
        backend_or_imposter,
        &[backend_or_imposter],
    )
}

fn init_drop(svm: &mut LiteSVM, admin: &Keypair, drop_id: u64, deadline: i64) {
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeDrop {
                config: config_pda(),
                admin: admin.pubkey(),
                drop: drop_pda(drop_id),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::InitializeDrop {
                drop_id,
                ticket_cost: 1,
                max_entries: 100,
                deadline_ts: deadline,
                prize_amount: 0,
            }
            .data(),
        },
        admin,
        &[admin],
    )
    .expect("initialize_drop");
}

fn enter(svm: &mut LiteSVM, user: &Keypair, drop_key: Pubkey, entry_count: u64) {
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::EnterDrop {
                config: config_pda(),
                user_signer: user.pubkey(),
                user_account: user_pda(&user.pubkey()),
                drop: drop_key,
                entry: entry_pda(&drop_key, &user.pubkey()),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::EnterDrop { entry_count }.data(),
        },
        user,
        &[user],
    )
    .expect("enter_drop");
}

fn close(svm: &mut LiteSVM, payer: &Keypair, drop_key: Pubkey) -> TransactionResult {
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::CloseEntries { drop: drop_key }.to_account_metas(None),
            data: cr_ix::CloseEntries {}.data(),
        },
        payer,
        &[payer],
    )
}

fn call_request_randomness(
    svm: &mut LiteSVM,
    drop_key: Pubkey,
    randomness_key: Pubkey,
) -> TransactionResult {
    // Use a fresh funded payer — anyone can call request_randomness.
    let payer = Keypair::new();
    fund(svm, &payer, 1);
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::RequestRandomness {
                drop: drop_key,
                randomness_account_data: randomness_key,
            }
            .to_account_metas(None),
            data: cr_ix::RequestRandomness {}.data(),
        },
        &payer,
        &[&payer],
    )
}

fn call_settle(
    svm: &mut LiteSVM,
    drop_key: Pubkey,
    randomness_key: Pubkey,
    winner: &Keypair,
    mint: Pubkey,
) -> TransactionResult {
    let payer = Keypair::new();
    fund(svm, &payer, 1);
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::SettleDrop {
                drop: drop_key,
                randomness_account_data: randomness_key,
                winner_entry: entry_pda(&drop_key, &winner.pubkey()),
                config: config_pda(),
                token_mint: mint,
                treasury_ata: treasury_ata(&mint),
                winner_token_account: user_ata_for(&winner.pubkey(), &mint),
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
            data: cr_ix::SettleDrop {}.data(),
        },
        &payer,
        &[&payer],
    )
}
