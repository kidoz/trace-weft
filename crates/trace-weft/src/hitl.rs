use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

lazy_static::lazy_static! {
    // Stores pending approvals: Span ID -> Sender to resume execution
    static ref PENDING_APPROVALS: Mutex<HashMap<String, oneshot::Sender<HitlResponse>>> = Mutex::new(HashMap::new());
}

#[derive(Debug, Clone)]
pub enum HitlResponse {
    Approved(Value),  // The potentially modified payload
    Rejected(String), // Reason for rejection
}

/// Registers a pending approval and returns a receiver to await the user's response.
pub fn register_approval(span_id: String) -> oneshot::Receiver<HitlResponse> {
    let (tx, rx) = oneshot::channel();
    if let Ok(mut pending) = PENDING_APPROVALS.lock() {
        pending.insert(span_id, tx);
    }
    rx
}

/// Resolves a pending approval with the given response.
pub fn resolve_approval(span_id: &str, response: HitlResponse) -> Result<(), String> {
    let tx = {
        if let Ok(mut pending) = PENDING_APPROVALS.lock() {
            pending.remove(span_id)
        } else {
            None
        }
    };

    if let Some(tx) = tx {
        tx.send(response)
            .map_err(|_| "Failed to send HITL response".to_string())
    } else {
        Err(format!("No pending approval found for span {}", span_id))
    }
}

/// Returns a list of all currently pending span IDs.
pub fn get_pending_approvals() -> Vec<String> {
    if let Ok(pending) = PENDING_APPROVALS.lock() {
        pending.keys().cloned().collect()
    } else {
        vec![]
    }
}
