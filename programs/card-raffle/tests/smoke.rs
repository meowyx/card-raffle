use card_raffle::ID;
use litesvm::LiteSVM;
use std::path::PathBuf;

#[test]
fn loads_program_into_litesvm() {
    let mut svm = LiteSVM::new();
    let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/card_raffle.so");
    svm.add_program_from_file(ID, &so_path)
        .expect("load card_raffle.so into LiteSVM");
}
