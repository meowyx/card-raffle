//! Lifecycle via credit_tickets (backend-signed) with prize_amount = 0.
//! See full_drop_with_prize.rs for the user-signed + on-chain payout path.

mod common;

use anchor_lang::{system_program, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl;
use card_raffle::{
    accounts as cr_accts, instruction as cr_ix, Config, DropStatus, Entry, PaymentReceipt, Raffle,
    UserAccount, ID,
};
use common::*;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

#[test]
fn full_drop_lifecycle() {
    let mut svm = setup_svm();

    let admin = Keypair::new();
    let backend = Keypair::new();
    let user_a = Keypair::new();
    let user_b = Keypair::new();
    for kp in [&admin, &backend, &user_a, &user_b] {
        fund(&mut svm, kp, 5);
    }

    let config = config_pda();
    let max_credit: u64 = 1_000_000;
    let mint = create_test_mint(&mut svm, &admin, &admin.pubkey());

    send_ix(
        &mut svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeConfig {
                admin: admin.pubkey(),
                config,
                token_mint: mint,
                treasury_ata: treasury_ata(&mint),
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::InitializeConfig {
                backend_signer: backend.pubkey(),
                max_credit_per_call: max_credit,
                max_credit_per_epoch: 50_000_000,
                ticket_price: 1_000_000,
            }
            .data(),
        },
        &admin,
        &[&admin],
    )
    .expect("initialize_config");

    let cfg = fetch::<Config>(&svm, &config);
    assert_eq!(cfg.admin, admin.pubkey());
    assert_eq!(cfg.backend_signer, backend.pubkey());

    let user_a_pda = user_pda(&user_a.pubkey());
    let user_b_pda = user_pda(&user_b.pubkey());
    init_user(&mut svm, &user_a, user_a_pda);
    init_user(&mut svm, &user_b, user_b_pda);

    credit_tickets(&mut svm, &backend, user_a_pda, "payment-A-10", 10);
    credit_tickets(&mut svm, &backend, user_b_pda, "payment-B-5", 5);

    let ua = fetch::<UserAccount>(&svm, &user_a_pda);
    assert_eq!(ua.ticket_balance, 10);
    let ub = fetch::<UserAccount>(&svm, &user_b_pda);
    assert_eq!(ub.ticket_balance, 5);

    let receipt = fetch::<PaymentReceipt>(&svm, &receipt_pda(&make_payment_id("payment-A-10")));
    assert_eq!(receipt.amount, 10);

    let drop_id: u64 = 1;
    let drop_key = drop_pda(drop_id);
    let deadline = now_ts(&svm) + 60;

    send_ix(
        &mut svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeDrop {
                config,
                admin: admin.pubkey(),
                drop: drop_key,
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
        &admin,
        &[&admin],
    )
    .expect("initialize_drop");

    let d = fetch::<Raffle>(&svm, &drop_key);
    assert_eq!(d.status, DropStatus::Open);
    assert_eq!(d.randomness_account, None);
    assert_eq!(d.commit_slot, 0);

    // A: 7 entries (indices 0..7), B: 3 entries (7..10).
    enter_drop(&mut svm, &user_a, user_a_pda, drop_key, 7);
    enter_drop(&mut svm, &user_b, user_b_pda, drop_key, 3);

    let d = fetch::<Raffle>(&svm, &drop_key);
    assert_eq!(d.total_entries, 10);

    let entry_a = fetch::<Entry>(&svm, &entry_pda(&drop_key, &user_a.pubkey()));
    assert_eq!(entry_a.start_index, 0);
    assert_eq!(entry_a.entry_count, 7);

    warp_clock(&mut svm, deadline + 1);
    send_ix(
        &mut svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::CloseEntries { drop: drop_key }.to_account_metas(None),
            data: cr_ix::CloseEntries {}.data(),
        },
        &admin,
        &[&admin],
    )
    .expect("close_entries");

    let d = fetch::<Raffle>(&svm, &drop_key);
    assert_eq!(d.status, DropStatus::Closed);

    // seed_slot = 99, current = 100 → fresh. reveal_slot = 0 → not yet revealed.
    let randomness_key = Pubkey::new_unique();
    warp_slot(&mut svm, 100);
    write_randomness(&mut svm, randomness_key, 99, 0, [0u8; 32]);

    send_ix(
        &mut svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::RequestRandomness {
                drop: drop_key,
                randomness_account_data: randomness_key,
            }
            .to_account_metas(None),
            data: cr_ix::RequestRandomness {}.data(),
        },
        &admin,
        &[&admin],
    )
    .expect("request_randomness");

    let d = fetch::<Raffle>(&svm, &drop_key);
    assert_eq!(d.status, DropStatus::RandomnessRequested);
    assert_eq!(d.randomness_account, Some(randomness_key));
    assert_eq!(d.commit_slot, 99);

    // ─── 8. settle_drop — mock the Switchboard reveal ──────────
    // 5 % 10 = 5, which falls in user_a's range [0, 7).
    let mut value = [0u8; 32];
    value[0..8].copy_from_slice(&5u64.to_le_bytes());
    write_randomness(&mut svm, randomness_key, 99, 200, value);
    warp_slot(&mut svm, 200);

    // Winner ATA required by settle_drop's context even when prize == 0.
    let _ = create_user_ata(&mut svm, &admin, &user_a.pubkey(), &mint);

    send_ix(
        &mut svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::SettleDrop {
                drop: drop_key,
                randomness_account_data: randomness_key,
                winner_entry: entry_pda(&drop_key, &user_a.pubkey()),
                config,
                token_mint: mint,
                treasury_ata: treasury_ata(&mint),
                winner_token_account: user_ata_for(&user_a.pubkey(), &mint),
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
            data: cr_ix::SettleDrop {}.data(),
        },
        &admin,
        &[&admin],
    )
    .expect("settle_drop");

    let d = fetch::<Raffle>(&svm, &drop_key);
    assert_eq!(d.status, DropStatus::Settled);
    assert_eq!(d.winner, Some(user_a.pubkey()));
    assert_eq!(d.winning_index, 5);
    assert_eq!(&d.random_value[0..8], &5u64.to_le_bytes());
}

// ─── Local helpers ────────────────────────────────────────────

fn fetch<T: AccountDeserialize>(svm: &litesvm::LiteSVM, pda: &Pubkey) -> T {
    let acct = svm.get_account(pda).expect("account exists");
    T::try_deserialize(&mut &acct.data[..]).expect("deserialize")
}

fn init_user(svm: &mut litesvm::LiteSVM, user: &Keypair, user_acct: Pubkey) {
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeUser {
                user_signer: user.pubkey(),
                user_account: user_acct,
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

fn credit_tickets(
    svm: &mut litesvm::LiteSVM,
    backend: &Keypair,
    user_acct: Pubkey,
    payment_label: &str,
    amount: u64,
) {
    let pid = make_payment_id(payment_label);
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::CreditTickets {
                config: config_pda(),
                backend_signer: backend.pubkey(),
                user_account: user_acct,
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
        backend,
        &[backend],
    )
    .expect("credit_tickets");
}

fn enter_drop(
    svm: &mut litesvm::LiteSVM,
    user: &Keypair,
    user_acct: Pubkey,
    drop_key: Pubkey,
    entry_count: u64,
) {
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::EnterDrop {
                config: config_pda(),
                user_signer: user.pubkey(),
                user_account: user_acct,
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
