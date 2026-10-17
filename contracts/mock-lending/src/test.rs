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
    lending: MockLendingClient<'a>,
}

fn setup<'a>() -> TestFixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let scoregate_id = env.register_contract(None, ScoreGateScoreContract);
    let scoregate = ScoreGateScoreContractClient::new(&env, &scoregate_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    scoregate.initialize(&admin, &service);

    let lending_id = env.register_contract(None, MockLending);
    let lending = MockLendingClient::new(&env, &lending_id);
    lending.initialize(&admin, &scoregate_id, &GATE_THRESHOLD, &MIN_CONFIDENCE);

    TestFixture { env, admin, scoregate, lending }
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
fn test_lending_set_admin_rotates_admin_and_emits_event() {
    let fixture = setup();
    let new_admin = Address::generate(&fixture.env);

    assert_eq!(fixture.lending.get_admin(), fixture.admin);

    let before_events = fixture.env.events().all().len();
    assert_eq!(fixture.lending.try_set_admin(&fixture.admin, &new_admin), Ok(Ok(())));
    assert_eq!(fixture.env.events().all().len(), before_events + 1);

    // Verify active admin updated
    assert_eq!(fixture.lending.get_admin(), new_admin);

    // Old admin can no longer perform administrative actions
    let alt_oracle = Address::generate(&fixture.env);
    assert_eq!(
        fixture.lending.try_set_risk_oracle(&fixture.admin, &alt_oracle),
        Err(Ok(MockLendingError::Unauthorized))
    );

    // New admin can perform administrative actions
    assert_eq!(fixture.lending.try_set_risk_oracle(&new_admin, &alt_oracle), Ok(Ok(())));
}

#[test]
fn test_lending_set_admin_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);
    let new_admin = Address::generate(&fixture.env);

    assert_eq!(
        fixture.lending.try_set_admin(&attacker, &new_admin),
        Err(Ok(MockLendingError::Unauthorized))
    );
    assert_eq!(fixture.lending.get_admin(), fixture.admin);
}

#[test]
fn test_lending_borrow_rejects_amount_exceeding_ceiling() {
    let fixture = setup();
    let borrower = Address::generate(&fixture.env);
    let pair = symbol_short!("XLM_USDC");
    submit_score(&fixture, &borrower, &pair, 10, 90);

    // Default max borrow amount
    assert_eq!(fixture.lending.get_max_borrow_amount(), DEFAULT_MAX_BORROW_AMOUNT);

    // Borrow within ceiling succeeds
    assert_eq!(fixture.lending.try_borrow(&borrower, &pair, &1_000), Ok(Ok(())));
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &pair, &DEFAULT_MAX_BORROW_AMOUNT),
        Ok(Ok(()))
    );

    // Amounts exceeding default ceiling are rejected with InvalidAmount
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &pair, &(DEFAULT_MAX_BORROW_AMOUNT + 1)),
        Err(Ok(MockLendingError::InvalidAmount))
    );
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &pair, &i128::MAX),
        Err(Ok(MockLendingError::InvalidAmount))
    );
}

#[test]
fn test_lending_set_max_borrow_amount_configuration() {
    let fixture = setup();
    let borrower = Address::generate(&fixture.env);
    let pair = symbol_short!("XLM_USDC");
    submit_score(&fixture, &borrower, &pair, 10, 90);

    // Admin updates ceiling to 5,000
    let before_events = fixture.env.events().all().len();
    assert_eq!(fixture.lending.try_set_max_borrow_amount(&fixture.admin, &5_000), Ok(Ok(())));
    assert_eq!(fixture.lending.get_max_borrow_amount(), 5_000);
    assert_eq!(fixture.env.events().all().len(), before_events + 1);

    // Borrow of 5,000 succeeds
    assert_eq!(fixture.lending.try_borrow(&borrower, &pair, &5_000), Ok(Ok(())));

    // Borrow of 5,001 fails
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &pair, &5_001),
        Err(Ok(MockLendingError::InvalidAmount))
    );

    // Non-positive ceiling is rejected
    assert_eq!(
        fixture.lending.try_set_max_borrow_amount(&fixture.admin, &0),
        Err(Ok(MockLendingError::InvalidAmount))
    );
    assert_eq!(
        fixture.lending.try_set_max_borrow_amount(&fixture.admin, &-100),
        Err(Ok(MockLendingError::InvalidAmount))
    );

    // Unauthorized caller cannot update ceiling
    let attacker = Address::generate(&fixture.env);
    assert_eq!(
        fixture.lending.try_set_max_borrow_amount(&attacker, &10_000),
        Err(Ok(MockLendingError::Unauthorized))
    );
}
