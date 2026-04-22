//! User-signed USDC ticket purchase. Counterpart to credit_tickets (backend-signed).

mod common;

use anchor_lang::{system_program, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl;
use card_raffle::{accounts as cr_accts, instruction as cr_ix, UserAccount, ID};
use common::*;
use litesvm::{types::TransactionResult, LiteSVM};
use solana_sdk::{
    instruction::{Instruction, InstructionError},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::TransactionError,
};

const ANCHOR_ERR_OFFSET: u32 = 6000;
const ERR_WRONG_MINT: u32 = 33;
/// Anchor built-in: `Account<'info, T>` owner mismatch.
const ANCHOR_ERR_OWNED_BY_WRONG_PROGRAM: u32 = 3007;

fn init_config_default(svm: &mut LiteSVM, admin: &Keypair, backend: &Keypair) -> Pubkey {
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
                max_credit_per_call: 1_000,
                max_credit_per_epoch: 10_000,
                ticket_price: usdc(19),
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

fn call_buy_tickets(
    svm: &mut LiteSVM,
    user: &Keypair,
    mint: &Pubkey,
    user_ata: &Pubkey,
    payment_label: &str,
    ticket_count: u64,
) -> TransactionResult {
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
}

fn fetch<T: AccountDeserialize>(svm: &LiteSVM, pda: &Pubkey) -> T {
    let acct = svm.get_account(pda).expect("account exists");
    T::try_deserialize(&mut &acct.data[..]).expect("deserialize")
}

fn assert_anchor_err(result: TransactionResult, expected_variant: u32) {
    assert_custom_err(result, ANCHOR_ERR_OFFSET + expected_variant);
}

/// Raw `Custom(code)` — no offset. For Anchor built-ins.
fn assert_custom_err(result: TransactionResult, expected_code: u32) {
    let failed = result.expect_err("tx should have failed");
    match failed.err {
        TransactionError::InstructionError(_, InstructionError::Custom(code)) => {
            assert_eq!(
                code, expected_code,
                "expected error code {}, got {} (logs: {:?})",
                expected_code, code, failed.meta.logs
            );
        }
        other => panic!("expected Custom error, got {:?}", other),
    }
}

#[test]
fn buy_tickets_succeeds() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    let user = Keypair::new();
    for kp in [&admin, &backend, &user] {
        fund(&mut svm, kp, 5);
    }

    let mint = init_config_default(&mut svm, &admin, &backend);
    init_user(&mut svm, &user);

    let user_ata = create_user_ata(&mut svm, &admin, &user.pubkey(), &mint);
    mint_to_ata(&mut svm, &admin, &mint, &user_ata, &admin, usdc(100));

    let t_ata = treasury_ata(&mint);
    assert_eq!(get_token_balance(&svm, &user_ata), usdc(100));
    assert_eq!(get_token_balance(&svm, &t_ata), 0);

    let ticket_count: u64 = 5;
    call_buy_tickets(&mut svm, &user, &mint, &user_ata, "buy-1", ticket_count)
        .expect("buy_tickets");

    // 5 × $19 = $95. User 100 − 95 = 5; treasury 0 + 95 = 95.
    let expected_total = usdc(19) * ticket_count;
    assert_eq!(get_token_balance(&svm, &user_ata), usdc(100) - expected_total);
    assert_eq!(get_token_balance(&svm, &t_ata), expected_total);

    let user_acct = fetch::<UserAccount>(&svm, &user_pda(&user.pubkey()));
    assert_eq!(user_acct.ticket_balance, ticket_count);
    assert_eq!(user_acct.total_purchased, ticket_count);
}

#[test]
fn buy_tickets_wrong_mint_fails() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    let user = Keypair::new();
    for kp in [&admin, &backend, &user] {
        fund(&mut svm, kp, 5);
    }

    let _real_mint = init_config_default(&mut svm, &admin, &backend);
    init_user(&mut svm, &user);

    // A mint never bound in Config.
    let fake_mint = create_test_mint(&mut svm, &admin, &admin.pubkey());
    let user_ata_fake = create_user_ata(&mut svm, &admin, &user.pubkey(), &fake_mint);
    mint_to_ata(&mut svm, &admin, &fake_mint, &user_ata_fake, &admin, usdc(100));
    let _ = create_user_ata(&mut svm, &admin, &config_pda(), &fake_mint);

    let result = call_buy_tickets(&mut svm, &user, &fake_mint, &user_ata_fake, "buy-fake", 1);
    assert_anchor_err(result, ERR_WRONG_MINT);
}

#[test]
fn initialize_config_rejects_token_2022_mint() {
    let mut svm = setup_svm();
    let admin = Keypair::new();
    let backend = Keypair::new();
    fund(&mut svm, &admin, 5);

    let fake_2022_mint = create_fake_token_2022_mint(&mut svm);
    let result = send_ix(
        &mut svm,
        Instruction {
            program_id: ID,
            accounts: cr_accts::InitializeConfig {
                admin: admin.pubkey(),
                config: config_pda(),
                token_mint: fake_2022_mint,
                treasury_ata: treasury_ata(&fake_2022_mint),
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: cr_ix::InitializeConfig {
                backend_signer: backend.pubkey(),
                max_credit_per_call: 1_000,
                max_credit_per_epoch: 10_000,
                ticket_price: usdc(19),
            }
            .data(),
        },
        &admin,
        &[&admin],
    );
    assert_custom_err(result, ANCHOR_ERR_OWNED_BY_WRONG_PROGRAM);
}
