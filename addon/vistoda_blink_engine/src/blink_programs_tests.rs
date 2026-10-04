//! Synthetic fixtures modeled on Android 59.1 `scheduling.Program`,
//! `ScheduleAction` (Gson field names) and `ProgramApi` routes.

use serde_json::json;

use super::{parse_programs, program_toggle_path, programs_path, valid_id};

fn fixture() -> serde_json::Value {
    json!([
        {
            "id": 4021, "network_id": 85507, "name": "Notte", "format": "v1",
            "status": "enabled",
            "schedule": [
                {"action": "arm", "time": "2026-10-04 20:00:00 +0000",
                 "dow": ["mon", "tue", "wed", "thu", "fri"], "devices": [11, 12]},
                {"action": "disarm", "time": "2026-10-05 05:30:00 +0000",
                 "dow": ["tue", "wed", "wed", "thu", "fri", "sat"], "devices": []}
            ]
        },
        {"id": "4022", "network_id": "85507", "name": "Weekend", "status": "disabled",
         "schedule": [{"action": "ARM", "time": "2026-10-03 08:00:00 +0000", "dow": ["saturday", "sun"]}]},
        {"id": 4023, "network_id": 85507, "name": "Senza stato"},
        {"id": 4024, "network_id": 99, "name": "Altra rete", "status": "enabled"},
        {"id": 0, "name": "Zero"},
        {"name": "Senza ID"}
    ])
}

#[test]
fn uses_the_native_program_routes() {
    assert_eq!(
        programs_path("9", "85507"),
        "/api/v1/accounts/9/networks/85507/programs"
    );
    assert_eq!(
        program_toggle_path("9", "85507", "4021", true),
        "/api/v1/accounts/9/networks/85507/programs/4021/enable"
    );
    assert_eq!(
        program_toggle_path("9", "85507", "4021", false),
        "/api/v1/accounts/9/networks/85507/programs/4021/disable"
    );
}

#[test]
fn parses_programs_like_the_official_app() -> Result<(), &'static str> {
    let programs = parse_programs(&fixture(), "85507").ok_or("list")?;
    let ids: Vec<_> = programs.iter().map(|program| program.id.as_str()).collect();
    assert_eq!(
        ids,
        ["4021", "4022", "4023"],
        "other networks and bad IDs are dropped"
    );
    let night = &programs[0];
    assert!(night.enabled);
    assert_eq!(night.name, "Notte");
    assert_eq!(night.schedule.len(), 2);
    assert_eq!(night.schedule[0].action, "arm");
    assert_eq!(night.schedule[0].days, ["mon", "tue", "wed", "thu", "fri"]);
    assert_eq!(night.schedule[0].device_count, 2);
    assert_eq!(
        night.schedule[0].time.as_deref(),
        Some("2026-10-04 20:00:00 +0000")
    );
    assert_eq!(night.schedule[1].action, "disarm");
    assert_eq!(
        night.schedule[1].days,
        ["tue", "wed", "thu", "fri", "sat"],
        "duplicates are removed"
    );
    // `Program.isEnabled()`: everything except "disabled" or a missing status.
    assert!(!programs[1].enabled);
    assert_eq!(programs[1].schedule[0].action, "arm");
    assert_eq!(programs[1].schedule[0].days, ["sat", "sun"]);
    assert!(!programs[2].enabled && programs[2].schedule.is_empty());
    Ok(())
}

#[test]
fn rejects_non_lists_and_bounds_untrusted_values() -> Result<(), &'static str> {
    assert!(parse_programs(&json!({"message": "nope"}), "1").is_none());
    let wrapped = parse_programs(&json!({"programs": [{"id": 5, "status": "enabled"}]}), "1");
    assert_eq!(wrapped.map(|list| list.len()), Some(1));
    let long = "x".repeat(500);
    let many: Vec<_> = (1..=100)
        .map(|id| json!({"id": id, "name": long, "schedule": vec![json!({"action": "boom", "time": long, "dow": ["xyz"]}); 50]}))
        .collect();
    let programs = parse_programs(&json!(many), "1").ok_or("list")?;
    assert_eq!(programs.len(), 32);
    assert_eq!(programs[0].name.chars().count(), 64);
    assert_eq!(programs[0].schedule.len(), 32);
    let action = &programs[0].schedule[0];
    assert_eq!(action.action, "none");
    assert!(action.time.is_none() && action.days.is_empty());
    let control = parse_programs(&json!([{"id": 1, "name": "a\u{0}\nb"}]), "1").ok_or("list")?;
    assert_eq!(control[0].name, "ab");
    Ok(())
}

#[test]
fn only_positive_numeric_ids_reach_a_url() {
    assert!(valid_id("4021"));
    for bad in [
        "",
        "0",
        "000",
        "12a",
        "../1",
        "1/enable",
        "123456789012345678901",
    ] {
        assert!(!valid_id(bad), "{bad}");
    }
}
