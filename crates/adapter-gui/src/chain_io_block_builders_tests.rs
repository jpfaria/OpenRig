    use super::*;
    use project::chain::{ChainInputMode, ChainOutputMode};

    fn input_draft(device: &str, channels: Vec<usize>, mode: ChainInputMode) -> InputGroupDraft {
        InputGroupDraft {
            device_id: Some(device.to_string()),
            channels,
            mode,
        }
    }

    fn output_draft(device: &str, channels: Vec<usize>, mode: ChainOutputMode) -> OutputGroupDraft {
        OutputGroupDraft {
            device_id: Some(device.to_string()),
            channels,
            mode,
        }
    }

    fn chain() -> ChainId {
        ChainId("chain-a".to_string())
    }

    #[test]
    fn build_input_block_returns_none_when_drafts_empty() {
        assert!(build_input_block_from_draft(&chain(), &[]).is_none());
    }

    #[test]
    fn build_input_block_collapses_two_devices_into_one_block_with_two_entries() {
        let drafts = vec![
            input_draft("dev-A", vec![0], ChainInputMode::Mono),
            input_draft("dev-B", vec![1], ChainInputMode::Mono),
        ];
        let block = build_input_block_from_draft(&chain(), &drafts).expect("block");
        assert_eq!(block.id.0, "chain-a:input", "deterministic block id");
        match &block.kind {
            AudioBlockKind::Input(ib) => {
                assert_eq!(
                    ib.entries.len(),
                    2,
                    "two devices → two entries in ONE block"
                );
                assert_eq!(ib.entries[0].device_id.0, "dev-A");
                assert_eq!(ib.entries[1].device_id.0, "dev-B");
            }
            other => panic!("expected InputBlock, got {:?}", other),
        }
    }

    #[test]
    fn build_input_block_preserves_per_entry_mode_and_channels() {
        let drafts = vec![
            input_draft("dev-mono", vec![0], ChainInputMode::Mono),
            input_draft("dev-stereo", vec![0, 1], ChainInputMode::Stereo),
            input_draft("dev-dual", vec![0, 1], ChainInputMode::DualMono),
        ];
        let block = build_input_block_from_draft(&chain(), &drafts).expect("block");
        let AudioBlockKind::Input(ib) = &block.kind else {
            panic!("expected InputBlock");
        };
        assert_eq!(ib.entries[0].mode, ChainInputMode::Mono);
        assert_eq!(ib.entries[0].channels, vec![0]);
        assert_eq!(ib.entries[1].mode, ChainInputMode::Stereo);
        assert_eq!(ib.entries[1].channels, vec![0, 1]);
        assert_eq!(ib.entries[2].mode, ChainInputMode::DualMono);
    }

    #[test]
    fn build_input_block_uses_empty_string_when_device_id_missing() {
        let drafts = vec![InputGroupDraft {
            device_id: None,
            channels: vec![0],
            mode: ChainInputMode::Mono,
        }];
        let block = build_input_block_from_draft(&chain(), &drafts).expect("block");
        let AudioBlockKind::Input(ib) = &block.kind else {
            panic!("expected InputBlock");
        };
        assert_eq!(ib.entries[0].device_id.0, "");
    }

    #[test]
    fn build_output_block_returns_none_when_drafts_empty() {
        assert!(build_output_block_from_draft(&chain(), &[]).is_none());
    }

    #[test]
    fn build_output_block_collapses_two_devices_into_one_block_with_two_entries() {
        let drafts = vec![
            output_draft("out-A", vec![0, 1], ChainOutputMode::Stereo),
            output_draft("out-B", vec![0], ChainOutputMode::Mono),
        ];
        let block = build_output_block_from_draft(&chain(), &drafts).expect("block");
        assert_eq!(block.id.0, "chain-a:output");
        match &block.kind {
            AudioBlockKind::Output(ob) => {
                assert_eq!(
                    ob.entries.len(),
                    2,
                    "two devices → two entries in ONE block"
                );
                assert_eq!(ob.entries[0].device_id.0, "out-A");
                assert_eq!(ob.entries[0].mode, ChainOutputMode::Stereo);
                assert_eq!(ob.entries[1].device_id.0, "out-B");
                assert_eq!(ob.entries[1].mode, ChainOutputMode::Mono);
            }
            other => panic!("expected OutputBlock, got {:?}", other),
        }
    }

    #[test]
    fn build_output_block_uses_standard_model_string() {
        let drafts = vec![output_draft("out-A", vec![0, 1], ChainOutputMode::Stereo)];
        let block = build_output_block_from_draft(&chain(), &drafts).expect("block");
        let AudioBlockKind::Output(ob) = &block.kind else {
            panic!("expected OutputBlock");
        };
        assert_eq!(ob.model, "standard");
    }
