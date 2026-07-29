use super::*;
use domain::ids::ChainId;

#[test]
fn a_frontend_that_hosts_nothing_answers_none_everywhere() {
    let live = NoLiveSource;
    assert!(live.chain_meters().is_none());
    assert!(live.tuner().is_none());
    assert!(live.spectrum().is_none());
    assert!(live.di_loop().is_none());
    assert!(live.chain_loopers(&ChainId("guitar".into())).is_none());
    assert!(live.devices().is_none());
    assert!(live.sample_rate().is_none());
}

#[test]
fn a_frontend_overrides_only_what_it_hosts() {
    struct MetersOnly;
    impl LiveSource for MetersOnly {
        fn chain_meters(&self) -> Option<Vec<ChainMeterReading>> {
            Some(vec![ChainMeterReading {
                chain: ChainId("guitar".into()),
                in_dbfs: -12.0,
                out_dbfs: -6.0,
            }])
        }
    }
    let live = MetersOnly;
    assert_eq!(live.chain_meters().unwrap().len(), 1);
    assert!(live.tuner().is_none(), "unimplemented reads stay None");
}

#[test]
fn a_hosted_device_source_can_report_that_enumeration_failed() {
    struct DeadHost;
    impl LiveSource for DeadHost {
        fn devices(&self) -> Option<Result<Vec<String>, String>> {
            Some(Err("no audio host".into()))
        }
    }
    // Three states, not two: not hosted / hosted-and-failed / hosted-and-ok.
    assert!(NoLiveSource.devices().is_none());
    assert_eq!(DeadHost.devices(), Some(Err("no audio host".to_string())));
}
