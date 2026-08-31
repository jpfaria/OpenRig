//! #913 — the parameter changes handed to a plugin's `process()`.
//!
//! The plugin walks these through raw COM pointers on the audio thread, so the
//! contract is not "returns the right number" — it is "never hands the plugin
//! something it will dereference into". An out-of-range index MUST answer null
//! and an out-of-range point MUST answer `kInvalidArgument`; either one getting
//! this wrong is undefined behaviour inside somebody else's binary.

use super::{HostParamValueQueue, HostParameterChanges};
use vst3::Steinberg::Vst::{IParamValueQueueTrait, IParameterChangesTrait};
use vst3::Steinberg::{kInvalidArgument, kResultOk};

#[test]
fn a_queue_reports_the_parameter_it_was_built_for() {
    let queue = HostParamValueQueue::new(42, 0.5);
    assert_eq!(unsafe { queue.getParameterId() }, 42);
}

#[test]
fn a_queue_holds_exactly_one_point() {
    let queue = HostParamValueQueue::new(1, 0.25);
    assert_eq!(unsafe { queue.getPointCount() }, 1);
}

#[test]
fn the_single_point_sits_at_the_start_of_the_block_with_its_value() {
    let queue = HostParamValueQueue::new(1, 0.75);
    let mut offset = -1i32;
    let mut value = -1.0f64;
    let result = unsafe { queue.getPoint(0, &mut offset, &mut value) };
    assert_eq!(result, kResultOk);
    assert_eq!(offset, 0, "the change applies from the first sample");
    assert_eq!(value, 0.75);
}

#[test]
fn asking_for_a_point_that_does_not_exist_is_refused_without_writing() {
    let queue = HostParamValueQueue::new(1, 0.75);
    let mut offset = -1i32;
    let mut value = -1.0f64;
    let result = unsafe { queue.getPoint(1, &mut offset, &mut value) };
    assert_eq!(
        result, kInvalidArgument,
        "answering kResultOk here would hand the plugin uninitialised memory"
    );
    assert_eq!(offset, -1, "the out-params must be left untouched");
    assert_eq!(value, -1.0);
}

#[test]
fn a_negative_index_is_refused_too() {
    let queue = HostParamValueQueue::new(1, 0.5);
    let mut offset = 0i32;
    let mut value = 0.0f64;
    assert_eq!(
        unsafe { queue.getPoint(-1, &mut offset, &mut value) },
        kInvalidArgument
    );
}

#[test]
fn a_plugin_writing_its_value_back_is_accepted_and_read_back() {
    // Some plugins write their current value into the host's queue.
    let queue = HostParamValueQueue::new(1, 0.1);
    let mut index = 0i32;
    assert_eq!(unsafe { queue.addPoint(0, 0.9, &mut index) }, kResultOk);
    let mut offset = 0i32;
    let mut value = 0.0f64;
    unsafe { queue.getPoint(0, &mut offset, &mut value) };
    assert_eq!(value, 0.9);
}

#[test]
fn the_collection_reports_one_queue_per_parameter() {
    let changes = HostParameterChanges::new(&[(1, 0.1), (2, 0.2), (3, 0.3)]);
    assert_eq!(unsafe { changes.getParameterCount() }, 3);
}

#[test]
fn an_empty_collection_reports_no_parameters() {
    let changes = HostParameterChanges::new(&[]);
    assert_eq!(unsafe { changes.getParameterCount() }, 0);
    assert!(unsafe { changes.getParameterData(0) }.is_null());
}

#[test]
fn every_advertised_index_yields_a_usable_queue() {
    let changes = HostParameterChanges::new(&[(7, 0.4), (8, 0.6)]);
    for index in 0..unsafe { changes.getParameterCount() } {
        assert!(
            !unsafe { changes.getParameterData(index) }.is_null(),
            "index {index} was advertised but has no queue"
        );
    }
}

#[test]
fn an_index_past_the_end_yields_null_rather_than_a_dangling_pointer() {
    let changes = HostParameterChanges::new(&[(1, 0.5)]);
    assert!(
        unsafe { changes.getParameterData(1) }.is_null(),
        "a non-null answer here is dereferenced by the plugin"
    );
    assert!(unsafe { changes.getParameterData(99) }.is_null());
    assert!(unsafe { changes.getParameterData(-1) }.is_null());
}

#[test]
fn the_plugin_cannot_add_data_to_the_hosts_input_changes() {
    // These are the host's INPUT changes: the plugin reads them, it does not
    // extend them. Handing back a queue would let it write into our block.
    let changes = HostParameterChanges::new(&[(1, 0.5)]);
    let id: u32 = 2;
    let mut index = 0i32;
    assert!(unsafe { changes.addParameterData(&id, &mut index) }.is_null());
}
