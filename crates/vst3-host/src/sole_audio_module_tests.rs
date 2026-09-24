use super::sole_audio_module_uid;
use crate::host::Vst3PluginClass;

fn class(uid: u8, name: &str, category: &str) -> Vst3PluginClass {
    Vst3PluginClass {
        uid: [uid; 16],
        name: name.to_string(),
        category: category.to_string(),
    }
}

#[test]
fn one_processor_with_its_controller_is_found() {
    // A bundle discovered by its folder name ("My Reverb") whose factory calls
    // the class "Reverb" (#978): the processor is still unambiguous.
    let classes = vec![
        class(1, "Reverb", "Audio Module Class"),
        class(2, "Reverb", "Component Controller Class"),
    ];
    assert_eq!(sole_audio_module_uid(&classes), Some([1; 16]));
}

#[test]
fn a_shell_with_several_processors_is_ambiguous() {
    let classes = vec![
        class(1, "Hall", "Audio Module Class"),
        class(2, "Plate", "Audio Module Class"),
    ];
    assert_eq!(sole_audio_module_uid(&classes), None);
}

#[test]
fn a_module_without_a_processor_has_none() {
    let classes = vec![class(2, "Reverb", "Component Controller Class")];
    assert_eq!(sole_audio_module_uid(&classes), None);
}
