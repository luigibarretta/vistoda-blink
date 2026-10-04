//! Blink arm/disarm schedules ("programs", Android 59.1 `ProgramApi`).
//!
//! Server-side programs keep running after the official app is removed and can
//! silently undo Home Assistant arming. Vistoda reads them and toggles their
//! enabled state only; create, update and delete stay out of scope (ADR 0014).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::blink_json::{owned_text, text, text_or_number};

/// Programs change only through the Blink app or Vistoda: read every 10 minutes.
pub const PROGRAM_REFRESH: Duration = Duration::from_secs(600);
const MAX_PROGRAMS: usize = 32;
const MAX_ACTIONS: usize = 32;
const MAX_NAME_CHARS: usize = 64;
const MAX_TIME_CHARS: usize = 40;
/// Blink stores `ScheduleAction.dow` as lower-case three-letter UTC weekdays.
const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// One Blink program as `GET v1/accounts/{a}/networks/{n}/programs` returns it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Program {
    pub id: String,
    pub network_id: String,
    pub name: String,
    /// The app treats every status except `disabled` (and a missing one) as on.
    pub enabled: bool,
    pub status: Option<String>,
    pub schedule: Vec<ScheduleAction>,
}

/// One `ScheduleAction`; Blink keeps time and weekdays in UTC.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScheduleAction {
    /// `arm`, `disarm` or `none` for an action Vistoda does not know.
    pub action: String,
    /// `yyyy-MM-dd HH:mm:ss Z`; the date anchors the UTC weekdays to local ones.
    pub time: Option<String>,
    pub days: Vec<String>,
    /// Cameras the action applies to (IDs are not exposed).
    pub device_count: usize,
}

pub fn programs_path(account: &str, network: &str) -> String {
    format!("/api/v1/accounts/{account}/networks/{network}/programs")
}

pub fn program_toggle_path(account: &str, network: &str, program: &str, enabled: bool) -> String {
    let verb = if enabled { "enable" } else { "disable" };
    format!("{}/{program}/{verb}", programs_path(account, network))
}

/// Blink IDs are positive integers; anything else never reaches a URL.
pub fn valid_id(value: &str) -> bool {
    (1..=20).contains(&value.len())
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.bytes().any(|byte| byte != b'0')
}

/// Parse the native list (also tolerating a `{"programs": [...]}` wrapper).
/// `None` means the body is not a program list at all.
pub fn parse_programs(value: &Value, network_id: &str) -> Option<Vec<Program>> {
    let items = value
        .as_array()
        .or_else(|| value.get("programs").and_then(Value::as_array))?;
    Some(
        items
            .iter()
            .filter_map(|item| parse_program(item, network_id))
            .take(MAX_PROGRAMS)
            .collect(),
    )
}

fn parse_program(item: &Value, network_id: &str) -> Option<Program> {
    let id = text_or_number(item, "id").filter(|id| valid_id(id))?;
    // A program of another network is never attributed to this one.
    if text_or_number(item, "network_id").is_some_and(|value| value != network_id) {
        return None;
    }
    let status = owned_text(item, "status").map(|value| bounded(&value, MAX_NAME_CHARS));
    let enabled = status
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case("disabled"));
    let name = text(item, "name").map_or_else(String::new, |name| bounded(name, MAX_NAME_CHARS));
    let schedule = item
        .get("schedule")
        .and_then(Value::as_array)
        .map(|actions| actions.iter().take(MAX_ACTIONS).map(parse_action).collect())
        .unwrap_or_default();
    Some(Program {
        id,
        network_id: network_id.to_owned(),
        name,
        enabled,
        status,
        schedule,
    })
}

fn parse_action(item: &Value) -> ScheduleAction {
    let action = match text(item, "action").map(str::to_ascii_lowercase).as_deref() {
        Some("arm") => "arm",
        Some("disarm") => "disarm",
        _ => "none",
    };
    let mut days: Vec<&str> = Vec::new();
    for day in item
        .get("dow")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let day = day.to_ascii_lowercase();
        if let Some(known) = DAYS.iter().find(|known| day.starts_with(**known))
            && !days.contains(known)
        {
            days.push(*known);
        }
    }
    days.sort_by_key(|day| DAYS.iter().position(|known| known == day));
    ScheduleAction {
        action: action.to_owned(),
        time: text(item, "time")
            .filter(|time| time.len() <= MAX_TIME_CHARS && time.is_ascii())
            .map(str::to_owned),
        days: days.into_iter().map(str::to_owned).collect(),
        device_count: item
            .get("devices")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    }
}

fn bounded(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

#[cfg(test)]
#[path = "blink_programs_tests.rs"]
mod tests;
