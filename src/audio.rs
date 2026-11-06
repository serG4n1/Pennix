//! Управление звуком через PulseAudio / PipeWire (pactl).
//! Позволяет точечно глушить звук только для скрываемых процессов (по PID).

use std::process::Command;

#[derive(Debug, Clone)]
pub struct SinkInput {
    pub id: u32,
    pub pid: Option<u32>,
    pub is_muted: bool,
}

/// Получает список всех активных аудиопотоков (sink-inputs).
pub fn list_sink_inputs() -> Vec<SinkInput> {
    let output = match Command::new("pactl")
        .arg("list")
        .arg("sink-inputs")
        .env("LC_ALL", "C")
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut results = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current_pid: Option<u32> = None;
    let mut current_muted = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink Input #") {
            if let Some(id) = current_id {
                results.push(SinkInput {
                    id,
                    pid: current_pid,
                    is_muted: current_muted,
                });
            }
            current_id = trimmed
                .trim_start_matches("Sink Input #")
                .trim()
                .parse::<u32>()
                .ok();
            current_pid = None;
            current_muted = false;
        } else if trimmed.starts_with("Mute:") {
            current_muted = trimmed.contains("yes");
        } else if trimmed.starts_with("application.process.id = \"") {
            if let Some(pid_str) = trimmed
                .strip_prefix("application.process.id = \"")
                .and_then(|s| s.strip_suffix('"'))
            {
                current_pid = pid_str.parse::<u32>().ok();
            }
        }
    }

    if let Some(id) = current_id {
        results.push(SinkInput {
            id,
            pid: current_pid,
            is_muted: current_muted,
        });
    }

    results
}

/// Глушит звук для указанных PID и возвращает список ID потоков, которые были замьючены.
pub fn mute_pids(pids: &[u32]) -> Vec<u32> {
    if pids.is_empty() {
        return Vec::new();
    }

    let streams = list_sink_inputs();
    let mut muted_ids = Vec::new();

    for stream in streams {
        if let Some(pid) = stream.pid {
            if pids.contains(&pid) && !stream.is_muted {
                let status = Command::new("pactl")
                    .args(["set-sink-input-mute", &stream.id.to_string(), "1"])
                    .status();
                if let Ok(st) = status {
                    if st.success() {
                        muted_ids.push(stream.id);
                    }
                }
            }
        }
    }

    muted_ids
}

/// Восстанавливает звук для ранее замьюченных потоков.
pub fn unmute_streams(stream_ids: &[u32]) {
    for id in stream_ids {
        let _ = Command::new("pactl")
            .args(["set-sink-input-mute", &id.to_string(), "0"])
            .status();
    }
}
