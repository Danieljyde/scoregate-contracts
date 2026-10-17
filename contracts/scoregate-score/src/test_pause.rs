#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::Address as _,
    Address, Env, Symbol, Vec,
};

use crate::{
    constants::MAX_ASSET_PAIR_LEN, Error, ScoreGateScoreContract,
    ScoreGateScoreContractClient,
};

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);

    (env, client, admin, service)
}

#[test]
fn test_set_pair_paused_symbol_length_bounded() {
    let (env, client, _admin, _service) = setup();
    assert_eq!(MAX_ASSET_PAIR_LEN, 9);

    // Bounded symbol (<= MAX_ASSET_PAIR_LEN = 9 characters) succeeds
    let valid_pair = symbol_short!("XLM_USDC");
    let res = client.try_set_pair_paused(&Vec::new(&env), &valid_pair, &true);
    assert!(res.is_ok());
    assert!(client.is_pair_paused(&valid_pair));

    // Exactly 9 bytes succeeds
    let max_len_pair = Symbol::new(&env, "123456789");
    let res = client.try_set_pair_paused(&Vec::new(&env), &max_len_pair, &true);
    assert!(res.is_ok());
    assert!(client.is_pair_paused(&max_len_pair));

    // Exceeding MAX_ASSET_PAIR_LEN (> 9 characters) returns AssetPairTooLong
    let too_long_pair = Symbol::new(&env, "VERY_LONG_ASSET_PAIR_NAME");
    let res = client.try_set_pair_paused(&Vec::new(&env), &too_long_pair, &true);
    assert_eq!(res, Err(Ok(Error::AssetPairTooLong)));

    // Storage is not polluted
    assert!(!client.is_pair_paused(&too_long_pair));
    let paused_pairs = client.get_paused_pairs();
    assert!(!paused_pairs.contains(&too_long_pair));
}

#[test]
fn test_set_pair_paused_multisig_enforcement() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.add_admin_signer(&Vec::new(&env), &s3);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let pair = symbol_short!("XLM_USDC");

    // Zero signers when threshold is 2 -> InsufficientAdminSigners
    let res = client.try_set_pair_paused(&Vec::new(&env), &pair, &true);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));
    assert!(!client.is_pair_paused(&pair));

    // One signer when threshold is 2 -> InsufficientAdminSigners
    let mut signers_1 = Vec::new(&env);
    signers_1.push_back(s1.clone());
    let res = client.try_set_pair_paused(&signers_1, &pair, &true);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));
    assert!(!client.is_pair_paused(&pair));

    // Two signers when threshold is 2 -> succeeds
    let mut signers_2 = Vec::new(&env);
    signers_2.push_back(s1.clone());
    signers_2.push_back(s2.clone());
    let res = client.try_set_pair_paused(&signers_2, &pair, &true);
    assert!(res.is_ok());
    assert!(client.is_pair_paused(&pair));

    // Unpause with quorum
    let res = client.try_set_pair_paused(&signers_2, &pair, &false);
    assert!(res.is_ok());
    assert!(!client.is_pair_paused(&pair));
}

#[test]
fn test_set_pair_paused_unauthorized_signer_rejected() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let rogue = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.set_admin_threshold(&Vec::new(&env), &1);

    let pair = symbol_short!("XLM_USDC");
    let mut signers = Vec::new(&env);
    signers.push_back(rogue);

    let res = client.try_set_pair_paused(&signers, &pair, &true);
    assert_eq!(res, Err(Ok(Error::AdminSignerNotInSet)));
    assert!(!client.is_pair_paused(&pair));
}
