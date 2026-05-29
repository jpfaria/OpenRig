use crate::{ir_model_schema, supported_models};

#[test]
fn supported_models_includes_generic_ir_loader() {
    // The native generic IR loader ("Impulse Response") lets the user load
    // a local `.wav` from disk. Regression guard for #603: it was deleted by
    // the #287 plugin move, emptying the registry and removing the tile.
    assert!(
        supported_models().contains(&"generic_ir"),
        "block-ir must register the native generic_ir loader"
    );
}

#[test]
fn supported_ir_models_expose_valid_schema() {
    for model in supported_models() {
        let schema = ir_model_schema(model).expect("schema should exist");
        assert_eq!(schema.effect_type, "ir");
        assert_eq!(schema.model, *model);
    }
}
