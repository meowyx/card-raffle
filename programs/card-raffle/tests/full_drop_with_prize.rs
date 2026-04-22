//! End-to-end: USDC purchase → drop entry → VRF settle → prize payout.
//! Token balances asserted at each step.

mod common;

use anchor_lang::{system_program, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl;
use card_raffle::{
    accounts as cr_accts, instruction as cr_ix, DropStatus, Raffle, UserAccount, ID,
};
use common::*;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

fn fetch<T: AccountDeserialize>(svm: &litesvm::LiteSVM, pda: &Pubkey) -> T {
    let acct = svm.get_account(pda).expect("account exists");
    T::try_deserialize(&mut &acct.data[..]).expect("deserialize")
}

#[test]
fn full_drop_with_prize_payout() {
    let mut svm = setup_svm();

    let admin = Keypair::new();
    let backend = Keypair::new();
    let user_a = Keypair::new();
    let user_b = Keypair::new();
    for kp in [&admin, &backend, &user_a, &user_b] {
        fund(&mut svm, kp, 5);
    }

    let config = config_pda();
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
                max_credit_per_call: 100,
                max_credit_per_epoch: 1_000,
                ticket_price: usdc(19),
            }
            .data(),
        },
        &admin,
        &[&admin],
    )
    .expect("initialize_config");

    let user_a_pda = user_pda(&user_a.pubkey());
    let user_b_pda = user_pda(&user_b.pubkey());
    for u in [&user_a, &user_b] {
        send_ix(
            &mut svm,
            Instruction {
                program_id: ID,
                accounts: cr_accts::InitializeUser {
                    user_signer: u.pubkey(),
                    user_account: user_pda(&u.pubkey()),
                    system_program: system_program::ID,
                }
                .to_account_metas(None),
                data: cr_ix::InitializeUser {}.data(),
            },
            u,
            &[u],
        )
        .expect("initialize_user");
    }

    // A buys 7 tickets ($133), B buys 3 ($57). Treasury accumulates $190.
    let user_a_ata = create_user_ata(&mut svm, &admin, &user_a.pubkey(), &mint);
    let user_b_ata = create_user_ata(&mut svm, &admin, &user_b.pubkey(), &mint);
    mint_to_ata(&mut svm, &admin, &mint, &user_a_ata, &admin, usdc(200));
    mint_to_ata(&mut svm, &admin, &mint, &user_b_ata, &admin, usdc(200));

    buy_tickets(&mut svm, &user_a, &mint, &user_a_ata, "a-buy", 7);
    buy_tickets(&mut svm, &user_b, &mint, &user_b_ata, "b-buy", 3);

    let t_ata = treasury_ata(&mint);
    assert_eq!(get_token_balance(&svm, &user_a_ata), usdc(200) - usdc(19) * 7);
    assert_eq!(get_token_balance(&svm, &user_b_ata), usdc(200) - usdc(19) * 3);
    assert_eq!(get_token_balance(&svm, &t_ata), usdc(19) * 10);

    let acct_a = fetch::<UserAccount>(&svm, &user_a_pda);
    let acct_b = fetch::<UserAccount>(&svm, &user_b_pda);
    assert_eq!(acct_a.ticket_balance, 7);
    assert_eq!(acct_b.ticket_balance, 3);

    // Prize = $100. Treasury has $190, so $90 stays after payout.
    let drop_id: u64 = 1;
    let drop_key = drop_pda(drop_id);
    let deadline = now_ts(&svm) + 60;
    let prize = usdc(100);

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
                prize_amount: prize,
            }
            .data(),
        },
        &admin,
        &[&admin],
    )
    .expect("initialize_drop");

    // A: 7 entries (0..7), B: 3 entries (7..10). Total 10.
    enter_drop(&mut svm, &user_a, user_a_pda, drop_key, 7);
    enter_drop(&mut svm, &user_b, user_b_pda, drop_key, 3);

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

    // 5 % 10 = 5 → in A's range [0, 7). A wins and receives the $100 prize.
    let mut value = [0u8; 32];
    value[0..8].copy_from_slice(&5u64.to_le_bytes());
    write_randomness(&mut svm, randomness_key, 99, 200, value);
    warp_slot(&mut svm, 200);

    let treasury_before = get_token_balance(&svm, &t_ata);
    let a_before = get_token_balance(&svm, &user_a_ata);

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
                treasury_ata: t_ata,
                winner_token_account: user_a_ata,
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
    assert_eq!(d.prize_amount, prize);

    assert_eq!(get_token_balance(&svm, &t_ata), treasury_before - prize);
    assert_eq!(get_token_balance(&svm, &user_a_ata), a_before + prize);
    assert_eq!(get_token_balance(&svm, &user_b_ata), usdc(200) - usdc(19) * 3);
}

// ─── Local helpers ────────────────────────────────────────────

fn buy_tickets(
    svm: &mut litesvm::LiteSVM,
    user: &Keypair,
    mint: &Pubkey,
    user_ata: &Pubkey,
    payment_label: &str,
    ticket_count: u64,
) {
    let payment_id = make_payment_id(payment_label);
    send_ix(
        svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::BuyTickets {
                config: config_pda(),
                user_signer: user.pubkey(),
                user_account: user_pda(&user.pubkey()),
                payment_receipt: receipt_pda(&payment_id),
                token_mint: *mint,
                user_token_account: *user_ata,
                treasury_ata: treasury_ata(mint),
                token_program: anchor_spl::token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::BuyTickets {
                payment_id,
                ticket_count,
            }
            .data(),
        },
        user,
        &[user],
    )
    .expect("buy_tickets");
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
