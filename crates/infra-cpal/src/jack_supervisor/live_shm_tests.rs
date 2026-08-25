use super::*;

fn server(name: &str) -> ServerName {
    ServerName::from(name)
}

#[test]
fn stale_entry_matches_the_servers_own_socket() {
    assert!(is_stale_entry(&server("openrig"), "jack_openrig_1000_0"));
}

#[test]
fn stale_entry_matches_the_servers_own_semaphore() {
    assert!(is_stale_entry(
        &server("openrig"),
        "jack_sem.1000_openrig_default"
    ));
}

#[test]
fn stale_entry_spares_another_servers_socket() {
    // The whole point of per-server cleanup: killing openrig's leftovers must
    // not touch a jackd someone else is running under a different name.
    assert!(!is_stale_entry(&server("openrig"), "jack_other_1000_0"));
}

#[test]
fn stale_entry_spares_another_servers_semaphore() {
    assert!(!is_stale_entry(
        &server("openrig"),
        "jack_sem.1000_other_default"
    ));
}

#[test]
fn stale_entry_spares_unrelated_files() {
    assert!(!is_stale_entry(&server("openrig"), "pulse-shm-12345"));
}

#[test]
fn a_server_name_that_prefixes_another_does_not_claim_its_socket() {
    // "rig" must not match "jack_rig2_1000_0" — the trailing underscore in the
    // prefix is what keeps the two apart.
    assert!(!is_stale_entry(&server("rig"), "jack_rig2_1000_0"));
    assert!(is_stale_entry(&server("rig"), "jack_rig_1000_0"));
}

#[test]
fn process_wide_entry_matches_the_registry_and_the_db() {
    assert!(is_process_wide_entry("jack-shm-registry"));
    assert!(is_process_wide_entry("jackdb_1000"));
    assert!(is_process_wide_entry("jack_db"));
}

#[test]
fn process_wide_entry_spares_a_running_servers_socket() {
    // "jack_<name>_<uid>_0" is a per-server socket, not a global file — the
    // global nuke must never take one down.
    assert!(!is_process_wide_entry("jack_openrig_1000_0"));
}

#[test]
fn process_wide_entry_spares_unrelated_files() {
    assert!(!is_process_wide_entry("pulse-shm-12345"));
}
