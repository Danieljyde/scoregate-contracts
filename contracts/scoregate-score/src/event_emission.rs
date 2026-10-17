#[cfg(test)]
mod test_event_schema {
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events as _},
        Address, Env, IntoVal, Symbol, Vec,
    };

    use crate::{events::EVENT_VERSION, ScoreGateScoreContract, ScoreGateScoreContractClient};

    #[test]
    fn test_all_events_carry_schema_version() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, ScoreGateScoreContract);
        let client = ScoreGateScoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let service = Address::generate(&env);

        // This triggers a subset of events (initialization, watch, etc.)
        client.initialize(&admin, &service);
        let wallet = Address::generate(&env);
        client.set_watchlist(&Vec::new(&env), &wallet, &true);

        let all_events = env.events().all();

        // Assert every single event emitted by this contract has EVENT_VERSION as its second topic.
        for (addr, topics, _data) in all_events.iter() {
            if addr == contract_id {
                assert!(topics.len() >= 2, "event topic array too short to contain schema version");
                // First topic is event name, second is schema version
                let version_topic: u32 = topics.get(1).unwrap().into_val(&env);
                assert_eq!(
                    version_topic, EVENT_VERSION,
                    "event missing correct schema version in topics"
                );
            }
        }
    }

    /// Regression test for `pair_weight_reset`'s versioned topic shape.
    ///
    /// `test_all_events_carry_schema_version` above never catches this
    /// because it never triggers `bulk_reset_pair_weight` (the only caller of
    /// `pair_weight_reset`). This test triggers that path and pins the
    /// versioned topic shape so future changes cannot silently
    /// break indexers.
    #[test]
    fn test_pair_weight_reset_carries_schema_version() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, ScoreGateScoreContract);
        let client = ScoreGateScoreContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let service = Address::generate(&env);
        client.initialize(&admin, &service);

        let pair = symbol_short!("XLM_USDC");
        client.set_pair_weight(&Vec::new(&env), &pair, &3);

        let mut pairs = Vec::new(&env);
        pairs.push_back(pair.clone());
        client.bulk_reset_pair_weight(&Vec::new(&env), &pairs);

        let all_events = env.events().all();
        let mut found_reset_event = false;
        for (addr, topics, _data) in all_events.iter() {
            if addr != contract_id {
                continue;
            }
            let name: Symbol = topics.get(0).unwrap().into_val(&env);
            if name != symbol_short!("pw_rst") {
                continue;
            }
            found_reset_event = true;
            assert!(
                topics.len() >= 2,
                "pair_weight_reset event topic array too short to contain schema version \
                 (found {} topics, expected at least 2)",
                topics.len()
            );
            let version_topic: u32 = topics.get(1).unwrap().into_val(&env);
            assert_eq!(
                version_topic, EVENT_VERSION,
                "pair_weight_reset event missing EVENT_VERSION in topics"
            );
        }
        assert!(
            found_reset_event,
            "bulk_reset_pair_weight did not emit a pair_weight_reset event as expected"
        );
    }

    /// Tests that all administrative and maintenance events updated in Issue #198
    /// emit EVENT_VERSION at topic index 1 and satisfy the Soroban <= 4 topics limit.
    #[test]
    fn test_administrative_events_carry_schema_version_and_topic_limits() {
        use crate::events::{self, EVENT_VERSION, MAX_EVENT_TOPICS};
        use soroban_sdk::{testutils::Events as _, BytesN};

        let env = Env::default();
        let admin = Address::generate(&env);
        let signer = Address::generate(&env);
        let root = BytesN::from_array(&env, &[42u8; 32]);

        let mut expected_symbols = Vec::new(&env);

        // 1. parameter_change_cleanup
        events::parameter_change_cleanup(&env, 10, 100);
        expected_symbols.push_back(symbol_short!("pc_clean"));

        // 2. model_version_proposed
        events::model_version_proposed(&env, 2, 1000);
        expected_symbols.push_back(symbol_short!("mv_prop"));

        // 3. model_version_activated
        events::model_version_activated(&env, 2);
        expected_symbols.push_back(symbol_short!("mv_act"));

        // 4. model_version_deprecated
        events::model_version_deprecated(&env, 1);
        expected_symbols.push_back(symbol_short!("mv_depr"));

        // 5. signer_tier_updated
        events::signer_tier_updated(&env, &signer, 10, 90);
        expected_symbols.push_back(symbol_short!("tier_upd"));

        // 6. adaptive_threshold_updated
        events::adaptive_threshold_updated(&env, 75);
        expected_symbols.push_back(symbol_short!("at_upd"));

        // 7. score_floor_policy_updated
        events::score_floor_policy_updated(&env, true, 80, 20);
        expected_symbols.push_back(symbol_short!("sf_upd"));

        // 8. finality_buffer_updated
        events::finality_buffer_updated(&env, 300);
        expected_symbols.push_back(symbol_short!("fb_upd"));

        // 9. heartbeat_threshold_updated
        events::heartbeat_threshold_updated(&env, 1800);
        expected_symbols.push_back(symbol_short!("hb_upd"));

        // 10. cluster_boundaries_updated
        events::cluster_boundaries_updated(&env);
        expected_symbols.push_back(symbol_short!("clb_upd"));

        // 11. epoch_opened
        events::epoch_opened(&env, 1);
        expected_symbols.push_back(symbol_short!("epo_open"));

        // 12. epoch_closed
        events::epoch_closed(&env, 1);
        expected_symbols.push_back(symbol_short!("epo_cls"));

        // 13. jump_threshold_updated
        events::jump_threshold_updated(&env, 50);
        expected_symbols.push_back(symbol_short!("jt_upd"));

        // 14. state_snapshot_created
        events::state_snapshot_created(&env, &root, 100, 500);
        expected_symbols.push_back(symbol_short!("snap"));

        // 15. contract_frozen
        events::contract_frozen(&env, &admin);
        expected_symbols.push_back(symbol_short!("frozen"));

        // 16. contract_unfrozen
        events::contract_unfrozen(&env, &admin);
        expected_symbols.push_back(symbol_short!("unfroz"));

        let all_events = env.events().all();
        assert_eq!(all_events.len(), expected_symbols.len() as u32);

        for (i, (_, topics, _)) in all_events.iter().enumerate() {
            let expected_sym = expected_symbols.get(i as u32).unwrap();
            let actual_sym: Symbol = topics.get(0).unwrap().into_val(&env);
            assert_eq!(actual_sym, expected_sym);

            // Assert topic arity <= Soroban limit of 4
            assert!(
                topics.len() <= MAX_EVENT_TOPICS as u32,
                "event topic count exceeds Soroban limit of 4"
            );
            // Assert EVENT_VERSION at topic index 1
            assert!(topics.len() >= 2, "event topic array too short to contain schema version");
            let version_topic: u32 = topics.get(1).unwrap().into_val(&env);
            assert_eq!(version_topic, EVENT_VERSION, "event missing EVENT_VERSION in topics");
        }
    }

    /// Unit test verifying that high-dimensional dispute events emit within host limits (Issue #201).
    #[test]
    fn test_dispute_events_emit_within_host_topic_limits() {
        use crate::events::{self, MAX_EVENT_TOPICS};
        use soroban_sdk::testutils::Events as _;

        let env = Env::default();
        let challenger = Address::generate(&env);
        let pair = symbol_short!("XLM_USDC");

        // 1. dispute_opened
        events::dispute_opened(&env, &challenger, &pair, 1_000_000, 1_700_100_000);
        let all = env.events().all();
        let (_, topics, data) = all.last().unwrap();
        assert!(topics.len() <= MAX_EVENT_TOPICS as u32, "dispute_opened exceeded max topic limit");
        let name: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(name, symbol_short!("disp_open"));
        let (d_pair, bond, deadline): (Symbol, i128, u64) = data.into_val(&env);
        assert_eq!((d_pair, bond, deadline), (pair.clone(), 1_000_000, 1_700_100_000));

        // 2. dispute_resolved
        events::dispute_resolved(&env, &challenger, &pair, 70, 1_000_000);
        let all = env.events().all();
        let (_, topics, data) = all.last().unwrap();
        assert!(
            topics.len() <= MAX_EVENT_TOPICS as u32,
            "dispute_resolved exceeded max topic limit"
        );
        let name: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(name, symbol_short!("disp_res"));
        let (d_pair, corrected, bond_ret): (Symbol, u32, i128) = data.into_val(&env);
        assert_eq!((d_pair, corrected, bond_ret), (pair.clone(), 70, 1_000_000));

        // 3. dispute_timed_out
        events::dispute_timed_out(&env, &challenger, &pair, 1_000_000, 50_000);
        let all = env.events().all();
        let (_, topics, data) = all.last().unwrap();
        assert!(
            topics.len() <= MAX_EVENT_TOPICS as u32,
            "dispute_timed_out exceeded max topic limit"
        );
        let name: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(name, symbol_short!("disp_to"));
        let (d_pair, bond, bonus): (Symbol, i128, i128) = data.into_val(&env);
        assert_eq!((d_pair, bond, bonus), (pair.clone(), 1_000_000, 50_000));
    }

    /// Unit test verifying compile-time trait guarantees on topic arity (Issue #201).
    #[test]
    fn test_valid_topic_tuple_arity_guarantees() {
        use crate::events::{assert_topic_limit, ValidTopicTuple, MAX_EVENT_TOPICS};

        assert!(<(u32,) as ValidTopicTuple>::TOPIC_COUNT <= MAX_EVENT_TOPICS);
        assert!(<(u32, u32) as ValidTopicTuple>::TOPIC_COUNT <= MAX_EVENT_TOPICS);
        assert!(<(u32, u32, u32) as ValidTopicTuple>::TOPIC_COUNT <= MAX_EVENT_TOPICS);
        assert!(<(u32, u32, u32, u32) as ValidTopicTuple>::TOPIC_COUNT <= MAX_EVENT_TOPICS);

        assert_topic_limit(&(1u32,));
        assert_topic_limit(&(1u32, 2u32));
        assert_topic_limit(&(1u32, 2u32, 3u32));
        assert_topic_limit(&(1u32, 2u32, 3u32, 4u32));
    }
}
