//! Regression tests for issue #142: follow_up_question must flush
//! intermediate text/handles before posting the question buttons.
//!
//! These tests verify the core invariant: all in-flight intermediate
//! sends complete BEFORE the question message is posted, preventing
//! the race where buttons appear above their context.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Mutex;

/// Simulate the intermediate_handles drain+await pattern used in all
/// four channel follow_up_question callbacks.
async fn drain_intermediates(handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>) {
    let drained: Vec<tokio::task::JoinHandle<()>> = {
        let mut guard = handles.lock().await;
        guard.drain(..).collect()
    };
    for h in drained {
        let _ = h.await;
    }
}

#[tokio::test]
async fn intermediates_complete_before_question_posts() {
    let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let intermediate_done = Arc::new(AtomicBool::new(false));

    let handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

    let order_clone = order.clone();
    let done_clone = intermediate_done.clone();
    let handle = tokio::spawn(async move {
        // Simulate slow intermediate text send
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        order_clone.lock().await.push("intermediate_sent");
        done_clone.store(true, Ordering::SeqCst);
    });
    handles.lock().await.push(handle);

    // Drain intermediates (same pattern as channel callbacks)
    drain_intermediates(handles.clone()).await;

    // Now "post the question"
    order.lock().await.push("question_posted");

    let final_order = order.lock().await.clone();
    assert_eq!(final_order, vec!["intermediate_sent", "question_posted"]);
    assert!(intermediate_done.load(Ordering::SeqCst));
}

#[tokio::test]
async fn multiple_intermediates_all_complete_before_question() {
    let completed = Arc::new(AtomicUsize::new(0));
    let handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

    for i in 0..5 {
        let done = completed.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10 * (i + 1))).await;
            done.fetch_add(1, Ordering::SeqCst);
        });
        handles.lock().await.push(handle);
    }

    drain_intermediates(handles.clone()).await;

    assert_eq!(completed.load(Ordering::SeqCst), 5);
    // After drain, the handles vec must be empty
    assert!(handles.lock().await.is_empty());
}

#[tokio::test]
async fn empty_handles_vec_does_not_block_question() {
    let handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

    // Should return immediately with no handles
    drain_intermediates(handles.clone()).await;

    assert!(handles.lock().await.is_empty());
}

#[tokio::test]
async fn drain_is_idempotent() {
    let handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

    let handle = tokio::spawn(async {});
    handles.lock().await.push(handle);

    // First drain
    drain_intermediates(handles.clone()).await;
    assert!(handles.lock().await.is_empty());

    // Second drain on empty vec should not panic or block
    drain_intermediates(handles.clone()).await;
    assert!(handles.lock().await.is_empty());
}

/// Verify that the sync Mutex pattern used in Discord/WhatsApp progress
/// callbacks (where the closure is synchronous) correctly captures handles
/// that the async question callback can later drain.
#[tokio::test]
async fn sync_mutex_captured_handles_drainable_async() {
    let sync_handles: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    // Simulate progress callback pushing handles (sync context)
    let handle = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    });
    sync_handles.lock().unwrap().push(handle);

    // Convert to async Mutex for drain (same pattern as question callback)
    let async_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = {
        let drained: Vec<tokio::task::JoinHandle<()>> =
            sync_handles.lock().unwrap().drain(..).collect();
        Arc::new(Mutex::new(drained))
    };

    drain_intermediates(async_handles.clone()).await;
    assert!(async_handles.lock().await.is_empty());
    assert!(sync_handles.lock().unwrap().is_empty());
}
