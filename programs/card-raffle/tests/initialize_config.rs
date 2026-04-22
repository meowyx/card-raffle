mod common;

use anchor_lang::system_program;
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl;
use card_raffle::{accounts as cr_accts, instruction as cr_ix, Config, ID};
use common::{create_test_mint, treasury_ata};
use litesvm::LiteSVM;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::path::PathBuf;

#[test]
fn initialize_config_writes_expected_state() {
    let mut svm = LiteSVM::new();
    let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/card_raffle.so");
    svm.add_program_from_file(ID, &so_path)
        .expect("load card_raffle.so");

    let admin = Keypair::new();
    let backend_signer = Keypair::new();
    svm.airdrop(&admin.pubkey(), 5_000_000_000).unwrap();

    let (config_pda, _bump) = Pubkey::find_program_address(&[b"config"], &ID);
    let max_credit: u64 = 1_000_000;
    let max_credit_per_epoch: u64 = 50_000_000;
    let ticket_price: u64 = 19_000_000;
    let mint = create_test_mint(&mut svm, &admin, &admin.pubkey());

    let ix = Instruction {
        program_id: ID,
        accounts: cr_accts::InitializeConfig {
            admin: admin.pubkey(),
            config: config_pda,
            token_mint: mint,
            treasury_ata: treasury_ata(&mint),
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: cr_ix::InitializeConfig {
            backend_signer: backend_signer.pubkey(),
            max_credit_per_call: max_credit,
            max_credit_per_epoch,
            ticket_price,
        }
        .data(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&admin.pubkey()),
        &[&admin],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "tx failed: {:?}", result.err());

    let cfg_account = svm
        .get_account(&config_pda)
        .expect("config account should exist after initialize_config");
    let cfg = Config::try_deserialize(&mut &cfg_account.data[..])
        .expect("deserialize Config");

    assert_eq!(cfg.admin, admin.pubkey());
    assert_eq!(cfg.backend_signer, backend_signer.pubkey());
    assert_eq!(cfg.max_credit_per_call, max_credit);
    assert_eq!(cfg.max_credit_per_epoch, max_credit_per_epoch);
    assert_eq!(cfg.current_epoch_total, 0);
    assert_eq!(cfg.current_epoch_start_ts, 0);
    assert_eq!(cfg.token_mint, mint);
    assert_eq!(cfg.ticket_price, ticket_price);
    assert_eq!(cfg.paused, false);
    assert_eq!(cfg.pending_admin, None);
}
