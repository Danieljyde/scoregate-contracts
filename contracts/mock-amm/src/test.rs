use super::*;
use scoregate_score::{ScoreGateScoreContract, ScoreGateScoreContractClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Vec,
};

const GATE_THRESHOLD: u32 = 75;
const MIN_CONFIDENCE: u32 = 50;

struct TestFixture<'a> {
    env: Env,
    admin: Address,
    scoregate: ScoreGateScoreContractClient<'a>,
    amm: MockAmmClient<'a>,
}

fn setup<'a>() -> TestFixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let scoregate_id = env.register_contract(None, ScoreGateScoreContract);
    let scoregate = ScoreGateScoreContractClient::new(&env, &scoregate_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    scoregate.initialize(&admin, &service);

    let amm_id = env.register_contract(None, MockAmm);
    let amm = MockAmmClient::new(&env, &amm_id);
    amm.initialize(&admin, &scoregate_id, &GATE_THRESHOLD);
    amm.set_liquidity_gate_config(
        &admin,
        &GATE_THRESHOLD,
        &MIN_CONFIDENCE,
        &FailPolicy::FailClosed,
        &604_800,
        &0,
    );

    TestFixture { env, admin, scoregate, amm }
}

fn submit_score(
    fixture: &TestFixture,
    wallet: &Address,
    pair: &soroban_sdk::Symbol,
    score: u32,
    confidence: u32,
) {
    fixture.env.ledger().with_mut(|l| l.timestamp += 3_601);
    fixture.scoregate.submit_score(
        &Vec::new(&fixture.env),
        wallet,
        pair,
        &score,
        &false,
        &false,
        &fixture.env.ledger().timestamp(),
        &confidence,
        &1,
        &None,
    );
}

#[test]
fn test_amm_set_admin_rotates_admin_and_emits_event() {
    let fixture = setup();
    let new_admin = Address::generate(&fixture.env);

    assert_eq!(fixture.amm.get_admin(), fixture.admin);

    let before_events = fixture.env.events().all().len();
    assert_eq!(fixture.amm.try_set_admin(&fixture.admin, &new_admin), Ok(Ok(())));
    assert_eq!(fixture.env.events().all().len(), before_events + 1);

    // Verify active admin is updated
    assert_eq!(fixture.amm.get_admin(), new_admin);

    // Old admin can no longer perform administrative actions
    let alt_oracle = Address::generate(&fixture.env);
    assert_eq!(
        fixture.amm.try_set_risk_oracle(&fixture.admin, &alt_oracle),
        Err(Ok(MockAmmError::Unauthorized))
    );

    // New admin can perform administrative actions
    assert_eq!(fixture.amm.try_set_risk_oracle(&new_admin, &alt_oracle), Ok(Ok(())));
}

#[test]
fn test_amm_set_admin_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);
    let new_admin = Address::generate(&fixture.env);

    assert_eq!(
        fixture.amm.try_set_admin(&attacker, &new_admin),
        Err(Ok(MockAmmError::Unauthorized))
    );
    assert_eq!(fixture.amm.get_admin(), fixture.admin);
}

#[test]
fn test_amm_pause_and_unpause_circuit_breaker() {
    let fixture = setup();
    let user = Address::generate(&fixture.env);
    let pair = symbol_short!("XLM_USDC");
    submit_score(&fixture, &user, &pair, 10, 90);

    // Verify initial unpaused state
    assert_eq!(fixture.amm.is_paused(), false);

    // Verify swap and provide_liquidity succeed when unpaused
    assert_eq!(fixture.amm.try_swap(&user, &pair, &1_000), Ok(Ok(())));
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&user, &pair, &1_000), Ok(Ok(())));

    // Pause the AMM
    let before_events = fixture.env.events().all().len();
    assert_eq!(fixture.amm.try_set_paused(&fixture.admin, &true), Ok(Ok(())));
    assert_eq!(fixture.amm.is_paused(), true);
    assert_eq!(fixture.env.events().all().len(), before_events + 1);

    // Operations are rejected with ContractPaused even though score is valid
    assert_eq!(fixture.amm.try_swap(&user, &pair, &1_000), Err(Ok(MockAmmError::ContractPaused)));
    assert_eq!(
        fixture.amm.try_provide_liquidity_gated(&user, &pair, &1_000),
        Err(Ok(MockAmmError::ContractPaused))
    );

    // Unpause restores operations
    assert_eq!(fixture.amm.try_set_paused(&fixture.admin, &false), Ok(Ok(())));
    assert_eq!(fixture.amm.is_paused(), false);
    assert_eq!(fixture.amm.try_swap(&user, &pair, &1_000), Ok(Ok(())));
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&user, &pair, &1_000), Ok(Ok(())));
}

#[test]
fn test_amm_set_paused_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);

    assert_eq!(fixture.amm.try_set_paused(&attacker, &true), Err(Ok(MockAmmError::Unauthorized)));
    assert_eq!(fixture.amm.is_paused(), false);
}

#[test]
fn test_amm_provide_liquidity_gated_multiple_distinct_pairs() {
    let fixture = setup();
    let provider = Address::generate(&fixture.env);
    let pair_xlm = symbol_short!("XLM_USDC");
    let pair_eurc = symbol_short!("EURC_USDC");

    // Provide safe score for XLM_USDC only
    submit_score(&fixture, &provider, &pair_xlm, 10, 90);

    // XLM_USDC succeeds
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&provider, &pair_xlm, &1_000), Ok(Ok(())));

    // EURC_USDC has no score submitted yet -> fails closed with HighRiskWallet
    assert_eq!(
        fixture.amm.try_provide_liquidity_gated(&provider, &pair_eurc, &1_000),
        Err(Ok(MockAmmError::HighRiskWallet))
    );

    // Submit safe score for EURC_USDC
    submit_score(&fixture, &provider, &pair_eurc, 15, 95);

    // Now EURC_USDC also succeeds
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&provider, &pair_eurc, &1_000), Ok(Ok(())));
}
