use super::*;

fn server(name: &str) -> ServerName {
    ServerName::from(name)
}

/// `/proc/<pid>/cmdline` is NUL-separated, so build the fixtures that way.
fn cmdline(args: &[&str]) -> String {
    let mut s = args.join("\0");
    s.push('\0');
    s
}

#[test]
fn matches_a_jackd_serving_the_named_server() {
    let c = cmdline(&["jackd", "-n", "openrig", "-d", "alsa"]);
    assert!(cmdline_is_jackd_for(&c, &server("openrig")));
}

#[test]
fn matches_jackd_invoked_by_absolute_path() {
    let c = cmdline(&["/usr/bin/jackd", "-n", "openrig", "-d", "alsa"]);
    assert!(cmdline_is_jackd_for(&c, &server("openrig")));
}

#[test]
fn rejects_a_jackd_serving_another_server() {
    let c = cmdline(&["jackd", "-n", "other", "-d", "alsa"]);
    assert!(!cmdline_is_jackd_for(&c, &server("openrig")));
}

#[test]
fn rejects_a_process_that_is_not_jackd() {
    // Someone else's argv can easily carry "-n openrig" — a text editor with
    // the name in its arguments must never be mistaken for our server.
    let c = cmdline(&["vim", "-n", "openrig", "notes.txt"]);
    assert!(!cmdline_is_jackd_for(&c, &server("openrig")));
}

#[test]
fn rejects_a_jackd_with_no_name_flag() {
    let c = cmdline(&["jackd", "-d", "alsa"]);
    assert!(!cmdline_is_jackd_for(&c, &server("openrig")));
}

#[test]
fn a_server_name_that_prefixes_another_does_not_claim_its_process() {
    // The dangerous one: this decides which PID gets SIGTERM/SIGKILL. If "rig"
    // matched a jackd running "rig2", terminate would kill the wrong server.
    let c = cmdline(&["jackd", "-n", "rig2", "-d", "alsa"]);
    assert!(!cmdline_is_jackd_for(&c, &server("rig")));
}

#[test]
fn the_exact_name_still_matches_when_a_longer_one_exists() {
    let c = cmdline(&["jackd", "-n", "rig", "-d", "alsa"]);
    assert!(cmdline_is_jackd_for(&c, &server("rig")));
}

#[test]
fn matches_when_the_name_is_the_last_argument() {
    let c = cmdline(&["jackd", "-d", "alsa", "-n", "openrig"]);
    assert!(cmdline_is_jackd_for(&c, &server("openrig")));
}
