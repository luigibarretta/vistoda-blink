//! Pipe-only child processes. The owner must drop/stop the codec on mute/close.
use std::{io, process::Stdio, time::Duration};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const PROGRAM: &str = "/usr/local/bin/ffmpeg";
const COMMON: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostdin",
    "-filter_threads",
    "1",
    "-filter_complex_threads",
    "1",
];
const ENCODER: &[&str] = &[
    "-f",
    "s16le",
    "-ar",
    "16000",
    "-ac",
    "1",
    "-blocksize",
    "1024",
    "-probesize",
    "32",
    "-analyzeduration",
    "0",
    "-i",
    "pipe:0",
    "-map",
    "0:a:0",
    "-c:a",
    "aac",
    "-profile:a",
    "aac_low",
    "-b:a",
    "32k",
    "-ar",
    "16000",
    "-ac",
    "1",
    "-threads",
    "1",
    "-f",
    "adts",
    "-flush_packets",
    "1",
    "pipe:1",
];

pub struct AudioCodec {
    child: Child,
    pub input: ChildStdin,
    pub output: ChildStdout,
}

impl AudioCodec {
    pub fn encoder() -> io::Result<Self> {
        Self::spawn(ENCODER)
    }

    fn spawn(arguments: &[&str]) -> io::Result<Self> {
        let mut child = command(arguments).spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing codec input"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing codec output"))?;
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub async fn stop(self) {
        let Self {
            mut child,
            input,
            output,
        } = self;
        // Kill, do not flush buffered speech after a microphone revocation.
        let _ = child.start_kill();
        drop(input);
        drop(output);
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
}

fn command(arguments: &[&str]) -> Command {
    let mut command = Command::new(PROGRAM);
    command
        .args(COMMON)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_commands_never_accept_network_or_user_paths() {
        let command = command(ENCODER);
        assert_eq!(command.as_std().get_program(), PROGRAM);
        let values: Vec<_> = command.as_std().get_args().collect();
        assert_eq!(values.iter().filter(|item| **item == "pipe:0").count(), 1);
        assert_eq!(values.iter().filter(|item| **item == "pipe:1").count(), 1);
        assert!(
            !values
                .iter()
                .any(|value| value.to_string_lossy().contains("://"))
        );
    }
}
