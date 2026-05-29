use crate::{nam_model_schema, supported_models};

#[test]
fn supported_models_includes_generic_nam_loader() {
    // The native generic NAM loader ("Neural Amp Modeler") lets the user
    // load a local `.nam` model from disk. Regression guard for #603: it was
    // deleted by the #287 plugin move, emptying the registry and removing
    // the tile.
    assert!(
        supported_models().contains(&"neural_amp_modeler"),
        "block-nam must register the native neural_amp_modeler loader"
    );
}

#[test]
fn supported_nam_models_expose_valid_schema() {
    for model in supported_models() {
        let schema = nam_model_schema(model).expect("schema should exist");
        assert_eq!(schema.effect_type, "nam");
        assert_eq!(schema.model, *model);
    }
}
