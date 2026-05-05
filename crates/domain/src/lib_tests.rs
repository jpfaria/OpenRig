use crate::ids::*;
use crate::value_objects::*;


// ── ChainId ──────────────────────────────────────────────────

#[test]
fn chain_id_generate_has_prefix() {
    let id = ChainId::generate();
    assert!(id.0.starts_with("chain:"), "expected 'chain:' prefix, got: {}", id.0);
}

#[test]
fn chain_id_generate_unique_each_call() {
    let a = ChainId::generate();
    let b = ChainId::generate();
    assert_ne!(a, b);
}

#[test]
fn chain_id_clone_equals_original() {
    let id = ChainId::generate();
    let cloned = id.clone();
    assert_eq!(id, cloned);
}

#[test]
fn chain_id_debug_contains_inner() {
    let id = ChainId("test-chain".into());
    let dbg = format!("{:?}", id);
    assert!(dbg.contains("test-chain"), "Debug output: {}", dbg);
}

#[test]
fn chain_id_hash_same_for_equal() {
    use std::collections::HashSet;
    let id = ChainId("same".into());
    let mut set = HashSet::new();
    set.insert(id.clone());
    assert!(set.contains(&id));
}

#[test]
fn chain_id_partial_eq_different_inner() {
    let a = ChainId("a".into());
    let b = ChainId("b".into());
    assert_ne!(a, b);
}

#[test]
fn chain_id_from_string_roundtrip() {
    let id = ChainId("custom-id".into());
    assert_eq!(id.0, "custom-id");
}

// ── BlockId ──────────────────────────────────────────────────

#[test]
fn block_id_generate_contains_chain_prefix() {
    let chain = ChainId("chain:abc".into());
    let block = BlockId::generate_for_chain(&chain);
    assert!(
        block.0.starts_with("chain:abc:block:"),
        "expected 'chain:abc:block:' prefix, got: {}",
        block.0
    );
}

#[test]
fn block_id_generate_unique_per_call() {
    let chain = ChainId::generate();
    let a = BlockId::generate_for_chain(&chain);
    let b = BlockId::generate_for_chain(&chain);
    assert_ne!(a, b);
}

#[test]
fn block_id_clone_equals_original() {
    let chain = ChainId::generate();
    let block = BlockId::generate_for_chain(&chain);
    let cloned = block.clone();
    assert_eq!(block, cloned);
}

#[test]
fn block_id_debug_contains_inner() {
    let block = BlockId("block-debug".into());
    let dbg = format!("{:?}", block);
    assert!(dbg.contains("block-debug"), "Debug output: {}", dbg);
}

#[test]
fn block_id_hash_same_for_equal() {
    use std::collections::HashSet;
    let id = BlockId("x".into());
    let mut set = HashSet::new();
    set.insert(id.clone());
    assert!(set.contains(&id));
}

// ── ParameterId ──────────────────────────────────────────────

#[test]
fn parameter_id_for_block_path_format() {
    let block = BlockId("chain:1:block:2".into());
    let param = ParameterId::for_block_path(&block, "gain");
    assert_eq!(param.0, "chain:1:block:2::gain");
}

#[test]
fn parameter_id_empty_path() {
    let block = BlockId("b".into());
    let param = ParameterId::for_block_path(&block, "");
    assert_eq!(param.0, "b::");
}

#[test]
fn parameter_id_clone_equals_original() {
    let param = ParameterId("p".into());
    assert_eq!(param.clone(), param);
}

#[test]
fn parameter_id_debug_contains_inner() {
    let param = ParameterId("debug-param".into());
    let dbg = format!("{:?}", param);
    assert!(dbg.contains("debug-param"));
}

#[test]
fn parameter_id_hash_same_for_equal() {
    use std::collections::HashSet;
    let id = ParameterId("p".into());
    let mut set = HashSet::new();
    set.insert(id.clone());
    assert!(set.contains(&id));
}

// ── DeviceId ─────────────────────────────────────────────────

#[test]
fn device_id_construct_and_access() {
    let dev = DeviceId("coreaudio:default".into());
    assert_eq!(dev.0, "coreaudio:default");
}

#[test]
fn device_id_clone_equals_original() {
    let dev = DeviceId("d".into());
    assert_eq!(dev.clone(), dev);
}

#[test]
fn device_id_debug_contains_inner() {
    let dev = DeviceId("dev-dbg".into());
    let dbg = format!("{:?}", dev);
    assert!(dbg.contains("dev-dbg"));
}

#[test]
fn device_id_hash_same_for_equal() {
    use std::collections::HashSet;
    let id = DeviceId("d".into());
    let mut set = HashSet::new();
    set.insert(id.clone());
    assert!(set.contains(&id));
}

#[test]
fn device_id_partial_eq_different() {
    let a = DeviceId("a".into());
    let b = DeviceId("b".into());
    assert_ne!(a, b);
}

// ── Normalized ───────────────────────────────────────────────

#[test]
fn normalized_construct_and_access() {
    let n = Normalized(0.5);
    assert_eq!(n.0, 0.5);
}

#[test]
fn normalized_clone_equals_original() {
    let n = Normalized(0.75);
    let c = n;
    assert_eq!(n, c);
}

#[test]
fn normalized_copy_semantics() {
    let n = Normalized(1.0);
    let c = n;
    // both still usable after copy
    assert_eq!(n.0, c.0);
}

#[test]
fn normalized_debug_format() {
    let n = Normalized(0.25);
    let dbg = format!("{:?}", n);
    assert!(dbg.contains("0.25"), "Debug: {}", dbg);
}

#[test]
fn normalized_partial_eq_different() {
    assert_ne!(Normalized(0.0), Normalized(1.0));
}

// ── Db ───────────────────────────────────────────────────────

#[test]
fn db_construct_and_access() {
    let d = Db(-3.0);
    assert_eq!(d.0, -3.0);
}

#[test]
fn db_copy_semantics() {
    let d = Db(6.0);
    let c = d;
    assert_eq!(d.0, c.0);
}

#[test]
fn db_debug_format() {
    let d = Db(-12.5);
    let dbg = format!("{:?}", d);
    assert!(dbg.contains("-12.5"), "Debug: {}", dbg);
}

#[test]
fn db_partial_eq_same() {
    assert_eq!(Db(0.0), Db(0.0));
}

#[test]
fn db_partial_eq_different() {
    assert_ne!(Db(0.0), Db(1.0));
}

// ── Hertz ────────────────────────────────────────────────────

#[test]
fn hertz_construct_and_access() {
    let h = Hertz(440.0);
    assert_eq!(h.0, 440.0);
}

#[test]
fn hertz_copy_semantics() {
    let h = Hertz(48000.0);
    let c = h;
    assert_eq!(h.0, c.0);
}

#[test]
fn hertz_debug_format() {
    let h = Hertz(440.0);
    let dbg = format!("{:?}", h);
    assert!(dbg.contains("440"), "Debug: {}", dbg);
}

#[test]
fn hertz_partial_eq() {
    assert_eq!(Hertz(440.0), Hertz(440.0));
    assert_ne!(Hertz(440.0), Hertz(441.0));
}

// ── Milliseconds ─────────────────────────────────────────────

#[test]
fn milliseconds_construct_and_access() {
    let ms = Milliseconds(100.0);
    assert_eq!(ms.0, 100.0);
}

#[test]
fn milliseconds_copy_semantics() {
    let ms = Milliseconds(50.0);
    let c = ms;
    assert_eq!(ms.0, c.0);
}

#[test]
fn milliseconds_debug_format() {
    let ms = Milliseconds(350.0);
    let dbg = format!("{:?}", ms);
    assert!(dbg.contains("350"), "Debug: {}", dbg);
}

#[test]
fn milliseconds_partial_eq() {
    assert_eq!(Milliseconds(10.0), Milliseconds(10.0));
    assert_ne!(Milliseconds(10.0), Milliseconds(20.0));
}

// ── ParameterValue ───────────────────────────────────────────

// Construction

#[test]
fn parameter_value_null_is_null() {
    assert!(ParameterValue::Null.is_null());
}

#[test]
fn parameter_value_bool_not_null() {
    assert!(!ParameterValue::Bool(true).is_null());
}

#[test]
fn parameter_value_int_not_null() {
    assert!(!ParameterValue::Int(42).is_null());
}

#[test]
fn parameter_value_float_not_null() {
    assert!(!ParameterValue::Float(1.0).is_null());
}

#[test]
fn parameter_value_string_not_null() {
    assert!(!ParameterValue::String("x".into()).is_null());
}

// as_bool

#[test]
fn parameter_value_as_bool_true() {
    assert_eq!(ParameterValue::Bool(true).as_bool(), Some(true));
}

#[test]
fn parameter_value_as_bool_false() {
    assert_eq!(ParameterValue::Bool(false).as_bool(), Some(false));
}

#[test]
fn parameter_value_as_bool_from_int_returns_none() {
    assert_eq!(ParameterValue::Int(1).as_bool(), None);
}

#[test]
fn parameter_value_as_bool_from_null_returns_none() {
    assert_eq!(ParameterValue::Null.as_bool(), None);
}

#[test]
fn parameter_value_as_bool_from_float_returns_none() {
    assert_eq!(ParameterValue::Float(1.0).as_bool(), None);
}

#[test]
fn parameter_value_as_bool_from_string_returns_none() {
    assert_eq!(ParameterValue::String("true".into()).as_bool(), None);
}

// as_i64

#[test]
fn parameter_value_as_i64_positive() {
    assert_eq!(ParameterValue::Int(42).as_i64(), Some(42));
}

#[test]
fn parameter_value_as_i64_negative() {
    assert_eq!(ParameterValue::Int(-100).as_i64(), Some(-100));
}

#[test]
fn parameter_value_as_i64_zero() {
    assert_eq!(ParameterValue::Int(0).as_i64(), Some(0));
}

#[test]
fn parameter_value_as_i64_from_float_returns_none() {
    assert_eq!(ParameterValue::Float(42.0).as_i64(), None);
}

#[test]
fn parameter_value_as_i64_from_null_returns_none() {
    assert_eq!(ParameterValue::Null.as_i64(), None);
}

#[test]
fn parameter_value_as_i64_from_bool_returns_none() {
    assert_eq!(ParameterValue::Bool(true).as_i64(), None);
}

#[test]
fn parameter_value_as_i64_from_string_returns_none() {
    assert_eq!(ParameterValue::String("42".into()).as_i64(), None);
}

// as_f32

#[test]
fn parameter_value_as_f32_from_float() {
    assert_eq!(ParameterValue::Float(3.14).as_f32(), Some(3.14));
}

#[test]
fn parameter_value_as_f32_from_int_coercion() {
    assert_eq!(ParameterValue::Int(10).as_f32(), Some(10.0));
}

#[test]
fn parameter_value_as_f32_from_negative_int() {
    assert_eq!(ParameterValue::Int(-5).as_f32(), Some(-5.0));
}

#[test]
fn parameter_value_as_f32_from_null_returns_none() {
    assert_eq!(ParameterValue::Null.as_f32(), None);
}

#[test]
fn parameter_value_as_f32_from_bool_returns_none() {
    assert_eq!(ParameterValue::Bool(false).as_f32(), None);
}

#[test]
fn parameter_value_as_f32_from_string_returns_none() {
    assert_eq!(ParameterValue::String("3.14".into()).as_f32(), None);
}

// as_str

#[test]
fn parameter_value_as_str_from_string() {
    let pv = ParameterValue::String("hello".into());
    assert_eq!(pv.as_str(), Some("hello"));
}

#[test]
fn parameter_value_as_str_empty_string() {
    let pv = ParameterValue::String(String::new());
    assert_eq!(pv.as_str(), Some(""));
}

#[test]
fn parameter_value_as_str_from_null_returns_none() {
    assert_eq!(ParameterValue::Null.as_str(), None);
}

#[test]
fn parameter_value_as_str_from_int_returns_none() {
    assert_eq!(ParameterValue::Int(1).as_str(), None);
}

#[test]
fn parameter_value_as_str_from_float_returns_none() {
    assert_eq!(ParameterValue::Float(1.0).as_str(), None);
}

#[test]
fn parameter_value_as_str_from_bool_returns_none() {
    assert_eq!(ParameterValue::Bool(true).as_str(), None);
}

// Clone and PartialEq

#[test]
fn parameter_value_clone_null() {
    let v = ParameterValue::Null;
    assert_eq!(v.clone(), v);
}

#[test]
fn parameter_value_clone_bool() {
    let v = ParameterValue::Bool(true);
    assert_eq!(v.clone(), v);
}

#[test]
fn parameter_value_clone_int() {
    let v = ParameterValue::Int(99);
    assert_eq!(v.clone(), v);
}

#[test]
fn parameter_value_clone_float() {
    let v = ParameterValue::Float(1.5);
    assert_eq!(v.clone(), v);
}

#[test]
fn parameter_value_clone_string() {
    let v = ParameterValue::String("cloned".into());
    assert_eq!(v.clone(), v);
}

#[test]
fn parameter_value_partial_eq_different_variants() {
    assert_ne!(ParameterValue::Null, ParameterValue::Bool(false));
    assert_ne!(ParameterValue::Int(0), ParameterValue::Float(0.0));
    assert_ne!(ParameterValue::Bool(true), ParameterValue::Int(1));
}

// Debug

#[test]
fn parameter_value_debug_null() {
    let dbg = format!("{:?}", ParameterValue::Null);
    assert!(dbg.contains("Null"), "Debug: {}", dbg);
}

#[test]
fn parameter_value_debug_bool() {
    let dbg = format!("{:?}", ParameterValue::Bool(true));
    assert!(dbg.contains("true"), "Debug: {}", dbg);
}

#[test]
fn parameter_value_debug_int() {
    let dbg = format!("{:?}", ParameterValue::Int(42));
    assert!(dbg.contains("42"), "Debug: {}", dbg);
}

#[test]
fn parameter_value_debug_float() {
    let dbg = format!("{:?}", ParameterValue::Float(2.5));
    assert!(dbg.contains("2.5"), "Debug: {}", dbg);
}

#[test]
fn parameter_value_debug_string() {
    let dbg = format!("{:?}", ParameterValue::String("abc".into()));
    assert!(dbg.contains("abc"), "Debug: {}", dbg);
}

// Edge cases

#[test]
fn parameter_value_as_f32_large_int() {
    let large = i64::MAX;
    let pv = ParameterValue::Int(large);
    assert!(pv.as_f32().is_some());
}

#[test]
fn parameter_value_as_f32_zero_int() {
    assert_eq!(ParameterValue::Int(0).as_f32(), Some(0.0));
}

#[test]
fn parameter_value_string_with_special_chars() {
    let pv = ParameterValue::String("coreaudio:default\n\t".into());
    assert_eq!(pv.as_str(), Some("coreaudio:default\n\t"));
}

#[test]
fn chain_id_empty_string() {
    let id = ChainId(String::new());
    assert_eq!(id.0, "");
}

#[test]
fn block_id_empty_chain_prefix() {
    let chain = ChainId(String::new());
    let block = BlockId::generate_for_chain(&chain);
    assert!(block.0.starts_with(":block:"), "got: {}", block.0);
}

#[test]
fn parameter_id_nested_path() {
    let block = BlockId("b1".into());
    let param = ParameterId::for_block_path(&block, "eq.band1.frequency");
    assert_eq!(param.0, "b1::eq.band1.frequency");
}
