use serde_json::Value;
use tracing::warn;

use crate::blink_client::BlinkError;

/// Vendor diagnostics are short; bound them so a hostile reply cannot flood logs.
const STATUS_MESSAGE_LIMIT: usize = 160;

/// Mirrors the native `SupervisorKommand.isSuccessful`: a command succeeds only
/// when it is complete and its numeric `status` is zero. `None` means pending.
pub fn completed(status: &Value) -> Option<Result<(), BlinkError>> {
    if status.get("complete").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let succeeded = match status.get("status") {
        // The native model defaults a missing status to zero.
        None | Some(Value::Null) => true,
        Some(Value::Number(code)) => code.as_i64() == Some(0),
        Some(Value::String(state)) => state != "failed",
        Some(_) => false,
    };
    if succeeded {
        return Some(Ok(()));
    }
    // Keep Blink's own reason: without it a rejected format or eject cannot be
    // diagnosed later (the vendor reply carries no credentials or identifiers).
    let message: String = status
        .get("status_msg")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(STATUS_MESSAGE_LIMIT)
        .collect();
    let code = status.get("status").cloned().unwrap_or(Value::Null);
    let status_code = status.get("status_code").and_then(Value::as_i64);
    warn!(
        status = %code,
        status_code = ?status_code,
        status_msg = %message,
        "Blink rejected a Sync Module command"
    );
    Some(Err(BlinkError::CommandFailed))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::completed;

    #[test]
    fn only_complete_zero_status_commands_succeed() {
        assert!(completed(&json!({"complete": false, "status": 0})).is_none());
        assert!(completed(&json!({"status": 0})).is_none());
        assert!(matches!(
            completed(&json!({"complete": true, "status": 0, "status_msg": "Command succeeded"})),
            Some(Ok(()))
        ));
        assert!(matches!(
            completed(&json!({"complete": true})),
            Some(Ok(()))
        ));
        for failed in [json!(1), json!(-1), json!("failed"), json!(true)] {
            assert!(matches!(
                completed(&json!({"complete": true, "status": failed})),
                Some(Err(crate::blink_client::BlinkError::CommandFailed))
            ));
        }
    }
}
