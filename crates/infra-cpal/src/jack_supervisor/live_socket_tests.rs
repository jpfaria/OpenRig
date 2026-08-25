use super::*;

fn server(name: &str) -> ServerName {
    ServerName::from(name)
}

#[test]
fn socket_entry_matches_the_named_servers_socket() {
    assert!(is_socket_entry(&server("openrig"), "jack_openrig_1000_0"));
}

#[test]
fn socket_entry_rejects_another_servers_socket() {
    assert!(!is_socket_entry(&server("openrig"), "jack_other_1000_0"));
}

#[test]
fn socket_entry_rejects_the_servers_semaphore() {
    // Semaphores share the prefix but not the "_0" suffix — treating one as a
    // live socket would make the supervisor believe a dead server is up.
    assert!(!is_socket_entry(
        &server("openrig"),
        "jack_sem.1000_openrig_default"
    ));
}

#[test]
fn a_server_name_that_prefixes_another_does_not_claim_its_socket() {
    assert!(!is_socket_entry(&server("rig"), "jack_rig2_1000_0"));
    assert!(is_socket_entry(&server("rig"), "jack_rig_1000_0"));
}

#[test]
fn any_socket_entry_matches_whatever_server_owns_it() {
    assert!(is_any_socket_entry("jack_openrig_1000_0"));
    assert!(is_any_socket_entry("jack_other_1000_0"));
}

#[test]
fn any_socket_entry_rejects_non_socket_files() {
    assert!(!is_any_socket_entry("jack-shm-registry"));
    assert!(!is_any_socket_entry("jack_sem.1000_openrig_default"));
    assert!(!is_any_socket_entry("pulse-shm-12345"));
}

#[test]
fn the_settling_window_is_shorter_than_the_poll_timeout() {
    // If settling ever outgrew the timeout the spawn path would sleep past its
    // own deadline before the first liveness check.
    assert!(POST_SOCKET_SETTLING < SOCKET_POLL_TIMEOUT);
}
