//! Which thread's allocations count as audio memory.

use super::{allocating_audio, audio_allocations, mark_audio_thread};

#[test]
fn a_thread_is_not_allocating_audio_by_default() {
    std::thread::spawn(|| assert!(!allocating_audio()))
        .join()
        .unwrap();
}

#[test]
fn a_scope_marks_the_thread_until_it_drops() {
    std::thread::spawn(|| {
        let scope = audio_allocations();
        assert!(allocating_audio(), "inside the scope");
        drop(scope);
        assert!(!allocating_audio(), "after the scope");
    })
    .join()
    .unwrap();
}

#[test]
fn a_nested_scope_restores_the_outer_one() {
    std::thread::spawn(|| {
        let outer = audio_allocations();
        drop(audio_allocations());
        assert!(
            allocating_audio(),
            "the inner scope ending must not unmark the outer one"
        );
        drop(outer);
        assert!(!allocating_audio());
    })
    .join()
    .unwrap();
}

#[test]
fn an_audio_thread_stays_marked_past_any_scope() {
    std::thread::spawn(|| {
        mark_audio_thread();
        drop(audio_allocations());
        assert!(allocating_audio());
    })
    .join()
    .unwrap();
}

#[test]
fn the_mark_belongs_to_one_thread() {
    std::thread::spawn(|| {
        let _scope = audio_allocations();
        std::thread::spawn(|| assert!(!allocating_audio()))
            .join()
            .unwrap();
    })
    .join()
    .unwrap();
}
