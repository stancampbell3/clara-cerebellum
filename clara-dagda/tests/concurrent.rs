use std::sync::{Arc, Barrier};
use std::thread;

use clara_dagda::{Dagda, TruthValue};
use uuid::Uuid;

/// 8 threads each write 50 distinct predicates; all 400 must be present after joining.
#[test]
fn concurrent_writes_no_data_races() {
    let dagda = Dagda::new().expect("Dagda::new");
    let session = Uuid::new_v4();
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();

    for thread_idx in 0u32..8 {
        let d = dagda.clone();
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            for i in 0u32..50 {
                let arg = format!("arg_{}_{}", thread_idx, i);
                d.set(session, "fact", &[&arg], TruthValue::KnownTrue)
                    .expect("set failed");
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let entries = dagda.list_session(session).expect("list_session");
    assert_eq!(entries.len(), 400, "expected 400 unique predicate entries");
}

/// Readers and writers run concurrently; verifies no panics and counts stay consistent.
#[test]
fn concurrent_mixed_read_write() {
    let dagda = Dagda::new().expect("Dagda::new");
    let session = Uuid::new_v4();

    // Pre-populate some entries
    for i in 0u32..20 {
        let arg = format!("pre_{i}");
        dagda.set(session, "base", &[&arg], TruthValue::KnownTrue)
            .expect("pre-populate set");
    }

    let barrier = Arc::new(Barrier::new(6));
    let mut handles = Vec::new();

    // 3 writer threads
    for thread_idx in 0u32..3 {
        let d = dagda.clone();
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            for i in 0u32..30 {
                let arg = format!("w_{}_{}", thread_idx, i);
                d.set(session, "dynamic", &[&arg], TruthValue::KnownUnresolved)
                    .expect("writer set failed");
            }
        }));
    }

    // 3 reader threads
    for _ in 0u32..3 {
        let d = dagda.clone();
        let b = barrier.clone();
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0u32..50 {
                // Just verify reads don't panic and return valid truth values
                let tv = d.get(session, "base", &["pre_0"]).expect("get failed");
                assert!(
                    matches!(
                        tv,
                        TruthValue::KnownTrue
                            | TruthValue::KnownFalse
                            | TruthValue::KnownUnresolved
                            | TruthValue::Unknown
                    ),
                    "unexpected truth value"
                );
                let _ = d.count_by_truth(session, TruthValue::KnownTrue)
                    .expect("count_by_truth failed");
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    // After all threads finish: 20 base + 90 dynamic = 110 total
    let total = dagda.list_session(session).expect("final list_session").len();
    assert_eq!(total, 110, "expected 110 entries after concurrent mixed load");
}
