//! A promoted worker leaves its realtime policy where the engine's split-path
//! threads can take it.

use super::promote_worker;

#[test]
fn a_promoted_worker_shares_its_policy_with_its_split_path_threads() {
    std::thread::spawn(|| {
        promote_worker(1_333_000, 1_100_000);
        assert!(engine::worker_rt_policy::promoter_installed());
        assert_eq!(
            engine::worker_rt_policy::thread_policy(),
            Some((1_333_000, 1_100_000))
        );
    })
    .join()
    .unwrap();
}
