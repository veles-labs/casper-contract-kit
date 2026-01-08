macro_rules! visitor {
    ($($symbol:tt)*) => {
        &[$(stringify!($symbol),)+][..]
    };
}

#[test]
fn it_works() {
    let pausable_symbols = veles_casper_contract_extras::enumerate_pausable_symbols!(visitor);
    assert_eq!(pausable_symbols, &["pause", "unpause", "is_paused",]);

    let ownable_symbols = veles_casper_contract_extras::enumerate_ownable_symbols!(visitor);
    assert_eq!(
        ownable_symbols,
        &["transfer_ownership", "renounce_ownership", "current_owner",]
    );
}
