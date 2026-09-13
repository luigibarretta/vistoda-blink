use super::*;
use crate::immi_audio_lease::AudioRuntime;
use std::error::Error;

#[tokio::test]
async fn commands_are_serialized_once_per_state() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, _frames) = runtime.connect_with_policy(Some(true));
    let mut uplink = Uplink::default();
    let mut bytes = Vec::new();

    let _status = runtime.request_recording(true)?;
    uplink.reconcile(&mut bytes, &connection).await?;
    uplink.reconcile(&mut bytes, &connection).await?;
    assert_eq!(bytes, SAVE);

    let _status = runtime.request_recording(false)?;
    uplink.reconcile(&mut bytes, &connection).await?;
    uplink.reconcile(&mut bytes, &connection).await?;
    assert_eq!(bytes, [SAVE, DISCARD].concat());
    Ok(())
}
