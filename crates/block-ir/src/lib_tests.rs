    use crate::{ir_model_schema, supported_models};

    #[test]
    fn supported_ir_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = ir_model_schema(model).expect("schema should exist");
            assert_eq!(schema.effect_type, "ir");
            assert_eq!(schema.model, *model);
        }
    }
