use serde_json::Value;

use crate::blink_client::BlinkError;

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
    Some(if succeeded {
        Ok(())
    } else {
        Err(BlinkError::CommandFailed)
    })
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
