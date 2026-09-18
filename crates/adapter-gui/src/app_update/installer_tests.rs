use super::{osascript_args, INSTALL_COMMAND};

#[test]
fn install_command_is_the_macos_installer_one_liner() {
    assert_eq!(
        INSTALL_COMMAND,
        "curl -fsSL https://raw.githubusercontent.com/jpfaria/OpenRig/develop/scripts/install-macos.sh | bash"
    );
}

#[test]
fn osascript_args_run_the_install_command_in_terminal() {
    let args = osascript_args();
    let script = args.join("\n");
    assert!(script.contains("tell application \"Terminal\""), "{script}");
    assert!(
        script.contains(&format!("do script \"{INSTALL_COMMAND}\"")),
        "{script}"
    );
    assert!(script.contains("activate"), "{script}");
    assert!(args.iter().step_by(2).all(|flag| flag == "-e"), "{args:?}");
}
