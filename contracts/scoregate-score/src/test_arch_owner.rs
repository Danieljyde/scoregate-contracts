#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    Address, Env, Vec,
};

use crate::{
    Error, ScoreGateScoreContract, ScoreGateScoreContractClient,
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
fn test_arch_owner_two_step_transfer_requires_acceptance() {
    let (env, client, _admin, _service) = setup();
    let arch_owner_1 = Address::generate(&env);
    let arch_owner_2 = Address::generate(&env);

    // Initial arch owner is None
    assert_eq!(client.get_arch_owner(), None);
    assert_eq!(client.get_pending_arch_owner(), None);

    // Step 1: Propose arch owner
    client.propose_arch_owner(&arch_owner_1);
    assert_eq!(client.get_pending_arch_owner(), Some(arch_owner_1.clone()));
    // Ownership has NOT changed yet (prevents single-step accidental transfer)
    assert_eq!(client.get_arch_owner(), None);

    // Step 2: Accept arch owner
    client.accept_arch_owner();
    assert_eq!(client.get_arch_owner(), Some(arch_owner_1.clone()));
    assert_eq!(client.get_pending_arch_owner(), None);

    // Transfer to arch_owner_2: propose
    client.propose_arch_owner(&arch_owner_2);
    assert_eq!(client.get_pending_arch_owner(), Some(arch_owner_2.clone()));
    // Current owner is still arch_owner_1 until accepted
    assert_eq!(client.get_arch_owner(), Some(arch_owner_1));

    // Accept transfer
    client.accept_arch_owner();
    assert_eq!(client.get_arch_owner(), Some(arch_owner_2));
    assert_eq!(client.get_pending_arch_owner(), None);
}

#[test]
fn test_accept_arch_owner_without_pending_fails() {
    let (_env, client, _admin, _service) = setup();
    let res = client.try_accept_arch_owner();
    assert_eq!(res, Err(Ok(Error::NoPendingAdminTransfer)));
}

#[test]
fn test_single_step_set_arch_owner_only_proposes() {
    let (env, client, _admin, _service) = setup();
    let candidate = Address::generate(&env);

    // Calling set_arch_owner only proposes candidate; does NOT immediately transfer
    client.set_arch_owner(&candidate);
    assert_eq!(client.get_arch_owner(), None);
    assert_eq!(client.get_pending_arch_owner(), Some(candidate.clone()));

    // Must be accepted to take effect
    client.accept_arch_owner();
    assert_eq!(client.get_arch_owner(), Some(candidate));
}

#[test]
fn test_admin_multisig_recovery_override_success() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    // Configure admin multisig
    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let lost_owner = Address::generate(&env);
    let mut admin_signers = Vec::new(&env);
    admin_signers.push_back(s1.clone());
    admin_signers.push_back(s2.clone());

    // Recover/initialize arch owner via admin multisig
    client.recover_arch_owner(&admin_signers, &lost_owner);
    assert_eq!(client.get_arch_owner(), Some(lost_owner));

    // Architecture owner key is lost! Admin multisig intervenes to recover ownership
    let recovered_owner = Address::generate(&env);
    let res = client.try_recover_arch_owner(&admin_signers, &recovered_owner);
    assert!(res.is_ok());
    assert_eq!(client.get_arch_owner(), Some(recovered_owner));
}

#[test]
fn test_admin_multisig_recovery_insufficient_signers_rejected() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let new_owner = Address::generate(&env);

    // 1 signer when threshold is 2 -> rejected
    let mut signers_1 = Vec::new(&env);
    signers_1.push_back(s1);

    let res = client.try_recover_arch_owner(&signers_1, &new_owner);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));
    assert_eq!(client.get_arch_owner(), None);
}
