//! Tests for chain_row_wiring pure handlers (issue #792 split).
    //! Issue #502: cover the pure handlers powering the Chains list
    //! ▲/▼ buttons. Selection-cursor reseating is tested via
    //! [`shift_selected_chain_index_after_swap`] in isolation; the
    //! Slint integration (calling `window.set_…`) lives in the
    //! wiring above and is exercised by the chained tests below.
    use super::*;
    use application::local_dispatcher::LocalDispatcher;
    use project::chain::Chain;
    use project::project::Project;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    fn make_chain(id: &str, description: &str) -> Chain {
        Chain {
            id: ChainId(id.into()),
            description: Some(description.into()),
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: Vec::new(),
            di_output: None,
        }
    }

    fn session_with_chains(rows: &[(&str, &str)]) -> ProjectSession {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: Vec::new(),
            chains: rows.iter().map(|(id, desc)| make_chain(id, desc)).collect(),
            midi: None,
        }));
        let dispatcher = Rc::new(LocalDispatcher::new(Rc::clone(&project)));
        ProjectSession {
            project,
            dispatcher,
            project_path: None,
            config_path: None,
            presets_path: PathBuf::from("./presets"),
            rig: None,
            io_bindings: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn chain_ids(session: &ProjectSession) -> Vec<String> {
        session
            .project
            .borrow()
            .chains
            .iter()
            .map(|c| c.id.0.clone())
            .collect()
    }

    #[test]
    fn apply_move_chain_up_swaps_session_chain_order() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_up(&session, 1)
            .expect("dispatcher ok")
            .expect("not a no-op");
        assert_eq!(outcome.moved_chain_id.0, "B");
        assert_eq!(outcome.previous_slot, 1);
        assert_eq!(outcome.new_slot, 0);
        assert_eq!(chain_ids(&session), vec!["B", "A"]);
    }

    #[test]
    fn apply_move_chain_up_at_slot_zero_is_noop() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_up(&session, 0).expect("dispatcher ok");
        assert!(outcome.is_none(), "moving slot 0 up is a no-op");
        assert_eq!(chain_ids(&session), vec!["A", "B"]);
    }

    #[test]
    fn apply_move_chain_up_invalid_slot_is_noop() {
        let session = session_with_chains(&[("A", "alpha")]);
        let outcome = apply_move_chain_up(&session, 99).expect("dispatcher ok");
        assert!(outcome.is_none(), "out-of-range slot returns None");
    }

    #[test]
    fn apply_move_chain_down_swaps_session_chain_order() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_down(&session, 0)
            .expect("dispatcher ok")
            .expect("not a no-op");
        assert_eq!(outcome.moved_chain_id.0, "A");
        assert_eq!(outcome.previous_slot, 0);
        assert_eq!(outcome.new_slot, 1);
        assert_eq!(chain_ids(&session), vec!["B", "A"]);
    }

    #[test]
    fn apply_move_chain_down_at_last_slot_is_noop() {
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta")]);
        let outcome = apply_move_chain_down(&session, 1).expect("dispatcher ok");
        assert!(outcome.is_none(), "moving last slot down is a no-op");
        assert_eq!(chain_ids(&session), vec!["A", "B"]);
    }

    #[test]
    fn apply_move_chain_down_invalid_slot_is_noop() {
        let session = session_with_chains(&[("A", "alpha")]);
        let outcome = apply_move_chain_down(&session, 99).expect("dispatcher ok");
        assert!(outcome.is_none(), "out-of-range slot returns None");
    }

    #[test]
    fn apply_move_chain_up_in_three_chain_project() {
        // The middle chain moves up; outcome reports it sat at slot 1 and
        // is now at slot 0 so the GUI can reseat the selection cursor.
        let session = session_with_chains(&[("A", "alpha"), ("B", "beta"), ("C", "gamma")]);
        let outcome = apply_move_chain_up(&session, 1)
            .expect("dispatcher ok")
            .expect("not a no-op");
        assert_eq!(outcome.moved_chain_id.0, "B");
        assert_eq!(outcome.new_slot, 0);
        assert_eq!(chain_ids(&session), vec!["B", "A", "C"]);
    }

    // ── selection cursor (no AppWindow) ──────────────────────────────────

    #[test]
    fn shift_selection_follows_moved_chain_on_up() {
        // User has chain at slot 1 selected; presses ▲ on that same chain.
        // The chain moves to slot 0 → cursor must follow to slot 0.
        let selected = 1;
        assert_eq!(
            shift_selected_chain_index_after_swap(selected, 1, 0),
            0,
            "cursor must follow the moved chain by ChainId, not stay on slot"
        );
    }

    #[test]
    fn shift_selection_follows_swapped_neighbour_on_up() {
        // User has chain at slot 0 selected; the user moves the chain at
        // slot 1 UP, which swaps slots 0 and 1. The originally-selected
        // chain is now at slot 1.
        let selected = 0;
        assert_eq!(
            shift_selected_chain_index_after_swap(selected, 1, 0),
            1,
            "the neighbour that got displaced must keep its ChainId selection"
        );
    }

    #[test]
    fn shift_selection_follows_moved_chain_on_down() {
        // User has chain at slot 0 selected; presses ▼ on it; chain moves
        // to slot 1 → cursor follows.
        let selected = 0;
        assert_eq!(shift_selected_chain_index_after_swap(selected, 0, 1), 1);
    }

    #[test]
    fn shift_selection_unaffected_for_unrelated_chain() {
        // User selected chain at slot 2; the move only swaps slots 0 and 1.
        let selected = 2;
        assert_eq!(
            shift_selected_chain_index_after_swap(selected, 0, 1),
            2,
            "an untouched slot's selection must not shift"
        );
    }

    #[test]
    fn shift_selection_preserves_no_selection_sentinel() {
        // `-1` is the Slint sentinel for "nothing selected"; it must not
        // be remapped.
        let selected = -1;
        assert_eq!(shift_selected_chain_index_after_swap(selected, 0, 1), -1);
    }
