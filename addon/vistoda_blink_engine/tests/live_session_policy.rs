use serde_json::{Value, json};
use vistoda_blink_engine::{
    blink_api::LiveDescriptor,
    framing::{ImmiDecoder, ImmiEvent},
};

#[test]
fn optional_policy_never_breaks_existing_video_descriptors() -> Result<(), serde_json::Error> {
    for policy in [
        json!(true),
        json!(false),
        Value::Null,
        json!(1),
        json!("true"),
        json!([]),
        json!({"enabled":true}),
    ] {
        let descriptor: LiveDescriptor = serde_json::from_value(json!({
            "server":"immis://fixture.invalid", "command_id":1,
            "is_mclv":policy,
        }))?;
        assert_eq!(descriptor.is_multi_client_live_view, policy.as_bool());
    }
    let descriptor: LiveDescriptor =
        serde_json::from_value(json!({"server":"fixture", "command_id":1}))?;
    assert_eq!(descriptor.is_multi_client_live_view, None);
    Ok(())
}

#[test]
fn only_native_top_level_wire_key_selects_audio_policy() -> Result<(), serde_json::Error> {
    for extra in [
        json!({"is_multi_client_live_view":true}),
        json!({"isMultiClientLiveViewSession":true}),
        json!({"options":{"is_mclv":true}}),
        json!({"response":{"is_mclv":true}}),
    ] {
        let mut response = json!({"server":"fixture", "command_id":1});
        if let (Some(fields), Some(extra)) = (response.as_object_mut(), extra.as_object()) {
            fields.extend(extra.clone());
        }
        let descriptor: LiveDescriptor = serde_json::from_value(response)?;
        assert_eq!(descriptor.is_multi_client_live_view, None);
    }
    // An unrelated internal-model name cannot override or break the wire field.
    for policy in [json!(true), json!(false), Value::Null, json!("true")] {
        let descriptor: LiveDescriptor = serde_json::from_value(json!({
            "server":"fixture", "command_id":1, "is_mclv":policy,
            "is_multi_client_live_view":true,
        }))?;
        assert_eq!(descriptor.is_multi_client_live_view, policy.as_bool());
    }
    Ok(())
}

#[test]
fn session_availability_is_passive_and_fragment_safe() -> Result<(), Box<dyn std::error::Error>> {
    for (id, available) in [(4_u32, true), (5, false)] {
        let mut frame = vec![0x18];
        frame.extend(id.to_be_bytes());
        frame.extend([0; 4]);
        for boundary in 0..=frame.len() {
            let mut decoder = ImmiDecoder::default();
            let mut events = decoder.push_events(&frame[..boundary])?;
            events.extend(decoder.push_events(&frame[boundary..])?);
            assert_eq!(events, vec![ImmiEvent::AudioAvailability(available)]);
            decoder.finish()?;
        }
        assert!(ImmiDecoder::default().push(&frame)?.is_empty());
        frame[8] = 1;
        frame.push(0);
        assert_eq!(
            ImmiDecoder::default().push_events(&frame)?,
            vec![ImmiEvent::AudioAvailability(available)]
        );
    }
    Ok(())
}
