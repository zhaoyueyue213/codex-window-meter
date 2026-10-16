use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(12);
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct QuotaWindow {
    pub remaining: u8,
    pub reset_at: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Quotas {
    pub short: Option<QuotaWindow>,
    pub weekly: Option<QuotaWindow>,
}

pub fn start(on_update: impl Fn(Option<Quotas>) + Send + 'static) {
    thread::spawn(move || {
        loop {
            if let Err(error) = read_session(&on_update) {
                eprintln!("quota reader: {error}");
                on_update(None);
            }
            thread::sleep(Duration::from_secs(8));
        }
    });
}

fn read_session(on_update: &impl Fn(Option<Quotas>)) -> Result<(), String> {
    let executable = std::env::var_os("CODEX_METER_CODEX_EXE").unwrap_or_else(|| "codex".into());
    let mut child = Command::new(executable)
        .args(["-s", "read-only", "-a", "never", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("cannot start codex app-server: {error}"))?;
    let result = communicate(&mut child, on_update);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn communicate(child: &mut Child, on_update: &impl Fn(Option<Quotas>)) -> Result<(), String> {
    let mut input = child.stdin.take().ok_or("app-server stdin unavailable")?;
    let output = child.stdout.take().ok_or("app-server stdout unavailable")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            match line {
                Ok(line) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });

    request(
        &mut input,
        1,
        "initialize",
        json!({
            "clientInfo": {"name": "codex-window-meter", "title": "Codex Window Meter", "version": "0.1.0"}
        }),
    )?;
    response(&receiver, 1)?;
    let mut request_id = 2;
    let mut last_refresh = Instant::now() - REFRESH_INTERVAL;
    loop {
        if last_refresh.elapsed() >= REFRESH_INTERVAL {
            request(
                &mut input,
                request_id,
                "account/rateLimits/read",
                Value::Null,
            )?;
            let value = response(&receiver, request_id)?;
            on_update(Some(parse_quotas(&value)?));
            request_id += 1;
            last_refresh = Instant::now();
        }
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                let value: Value =
                    serde_json::from_str(&line).map_err(|error| error.to_string())?;
                if value.get("method").and_then(Value::as_str) == Some("account/rateLimits/updated")
                {
                    last_refresh = Instant::now() - REFRESH_INTERVAL;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("app-server disconnected".into());
            }
        }
    }
}

fn request(input: &mut impl Write, id: u64, method: &str, params: Value) -> Result<(), String> {
    serde_json::to_writer(
        &mut *input,
        &json!({"id": id, "method": method, "params": params}),
    )
    .map_err(|error| error.to_string())?;
    input.write_all(b"\n").map_err(|error| error.to_string())?;
    input.flush().map_err(|error| error.to_string())
}

fn response(receiver: &Receiver<String>, id: u64) -> Result<Value, String> {
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    loop {
        let line = receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|error| format!("app-server response: {error}"))?;
        let value: Value = serde_json::from_str(&line).map_err(|error| error.to_string())?;
        if value.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("app-server error: {error}"));
        }
        return Ok(value);
    }
}

fn parse_quotas(value: &Value) -> Result<Quotas, String> {
    let limits = value
        .pointer("/result/rateLimits")
        .ok_or("missing rateLimits")?;
    let quotas = Quotas {
        short: parse_window(limits.get("primary")),
        weekly: parse_window(limits.get("secondary")),
    };
    if quotas.short.is_none() && quotas.weekly.is_none() {
        return Err("no active quota windows".into());
    }
    Ok(quotas)
}

fn parse_window(value: Option<&Value>) -> Option<QuotaWindow> {
    let value = value?.as_object()?;
    let used = value.get("usedPercent")?.as_f64()?.clamp(0.0, 100.0);
    Some(QuotaWindow {
        remaining: (100.0 - used).round() as u8,
        reset_at: value.get("resetsAt").and_then(Value::as_i64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_windows() {
        let value = json!({"result": {"rateLimits": {
            "primary": {"usedPercent": 8.5, "resetsAt": 1800000000},
            "secondary": {"usedPercent": 25, "resetsAt": 1800100000}
        }}});
        let quotas = parse_quotas(&value).unwrap();
        assert_eq!(quotas.short.unwrap().remaining, 92);
        assert_eq!(quotas.weekly.unwrap().remaining, 75);
    }

    #[test]
    fn accepts_one_window() {
        let value = json!({"result": {"rateLimits": {
            "primary": null,
            "secondary": {"usedPercent": 10, "resetsAt": 1800000000}
        }}});
        let quotas = parse_quotas(&value).unwrap();
        assert!(quotas.short.is_none());
        assert_eq!(quotas.weekly.unwrap().remaining, 90);
    }
}
