#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal,
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
fn test_set_score_delegate_dual_auth_success() {
    let (env, client, admin, _service) = setup();
    let sub_wallet = Address::generate(&env);
    let custodian = Address::generate(&env);

    // Both admin and sub_wallet authorize the delegation
    client
        .mock_auths(&[
            MockAuth {
                address: &admin,
                invoke: &MockAuthInvoke {
                    contract: &client.address,
                    fn_name: "set_score_delegate",
                    args: (sub_wallet.clone(), custodian.clone()).into_val(&env),
                    sub_invokes: &[],
                },
            },
            MockAuth {
                address: &sub_wallet,
                invoke: &MockAuthInvoke {
                    contract: &client.address,
                    fn_name: "set_score_delegate",
                    args: (sub_wallet.clone(), custodian.clone()).into_val(&env),
                    sub_invokes: &[],
                },
            },
        ])
        .set_score_delegate(&sub_wallet, &custodian);

    assert_eq!(client.get_score_delegate(&sub_wallet), Some(custodian));
}

#[test]
#[should_panic]
fn test_set_score_delegate_missing_sub_wallet_auth_fails() {
    let (env, client, admin, _service) = setup();
    let sub_wallet = Address::generate(&env);
    let custodian = Address::generate(&env);

    // Only admin authorizes the transaction; sub_wallet does NOT authorize.
    // Must panic because sub_wallet.require_auth() is enforced.
    client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "set_score_delegate",
                args: (sub_wallet.clone(), custodian.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .set_score_delegate(&sub_wallet, &custodian);
}

#[test]
#[should_panic]
fn test_set_score_delegate_missing_admin_auth_fails() {
    let (env, client, _admin, _service) = setup();
    let sub_wallet = Address::generate(&env);
    let custodian = Address::generate(&env);

    // Only sub_wallet authorizes, admin does not -> panics
    client
        .mock_auths(&[MockAuth {
            address: &sub_wallet,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "set_score_delegate",
                args: (sub_wallet.clone(), custodian.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .set_score_delegate(&sub_wallet, &custodian);
}

#[test]
fn test_cyclic_delegation_direct_self_delegation_rejected() {
    let (env, client, _admin, _service) = setup();
    env.mock_all_auths();
    let wallet_a = Address::generate(&env);

    let res = client.try_set_score_delegate(&wallet_a, &wallet_a);
    assert_eq!(res, Err(Ok(Error::CyclicDelegation)));
}

#[test]
fn test_cyclic_delegation_two_step_cycle_rejected() {
    let (env, client, _admin, _service) = setup();
    env.mock_all_auths();
    let wallet_a = Address::generate(&env);
    let wallet_b = Address::generate(&env);

    client.set_score_delegate(&wallet_a, &wallet_b);
    let res = client.try_set_score_delegate(&wallet_b, &wallet_a);
    assert_eq!(res, Err(Ok(Error::CyclicDelegation)));
}

#[test]
fn test_cyclic_delegation_transitive_cycle_rejected() {
    let (env, client, _admin, _service) = setup();
    env.mock_all_auths();
    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    let w3 = Address::generate(&env);

    client.set_score_delegate(&w1, &w2);
    client.set_score_delegate(&w2, &w3);

    // w3 -> w1 creates 3-cycle w1 -> w2 -> w3 -> w1
    let res = client.try_set_score_delegate(&w3, &w1);
    assert_eq!(res, Err(Ok(Error::CyclicDelegation)));
}

#[test]
fn test_remove_score_delegate_success() {
    let (env, client, _admin, _service) = setup();
    env.mock_all_auths();
    let sub_wallet = Address::generate(&env);
    let custodian = Address::generate(&env);

    client.set_score_delegate(&sub_wallet, &custodian);
    assert_eq!(client.get_score_delegate(&sub_wallet), Some(custodian));

    client.remove_score_delegate(&sub_wallet);
    assert_eq!(client.get_score_delegate(&sub_wallet), None);
}
