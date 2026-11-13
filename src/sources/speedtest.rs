//! Optional internet speed test against Cloudflare's speed endpoints.
//!
//! Runs only when explicitly requested (`--speedtest` or `s` in the TUI).
//! TLS is handled by rustls with baked-in webpki roots, so the static
//! binary needs no system CA certificates.

use std::time::{Duration, Instant};

const DOWN_URL: &str = "https://speed.cloudflare.com/__down?bytes={n}";
const UP_URL: &str = "https://speed.cloudflare.com/__up";
const CHUNK: usize = 65536;

#[derive(Clone, Default)]
pub struct SpeedResult {
    pub ping_ms: f64,
    pub down_mbps: f64,
    pub up_mbps: f64,
    pub error: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()
}

/// Round-trip time of a tiny request, in ms.
fn ping(agent: &ureq::Agent) -> Result<f64, String> {
    let url = DOWN_URL.replace("{n}", "10");
    let start = Instant::now();
    let resp = agent
        .get(&url)
        .call()
        .map_err(|e| format!("no connectivity ({e})"))?;
    let _ = resp
        .into_string()
        .map_err(|e| format!("read failed ({e})"))?;
    Ok((start.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0)
}

/// Stream-read the response for at most `seconds`; returns (bytes, elapsed).
fn drain(resp: ureq::Response, seconds: f64) -> (u64, f64) {
    let mut total = 0u64;
    let start = Instant::now();
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; CHUNK];
    loop {
        match std::io::Read::read(&mut reader, &mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                if start.elapsed().as_secs_f64() > seconds {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (total, start.elapsed().as_secs_f64())
}

fn download(agent: &ureq::Agent, seconds: f64) -> Result<f64, String> {
    let mut speed = 0.0f64;
    for size in [1_000_000u64, 10_000_000, 50_000_000] {
        let url = DOWN_URL.replace("{n}", &size.to_string());
        let resp = match agent.get(&url).call() {
            Ok(r) => r,
            Err(e) => {
                if speed > 0.0 {
                    break;
                }
                return Err(format!("download failed ({e})"));
            }
        };
        let (total, elapsed) = drain(resp, seconds);
        if elapsed > 0.2 && total > 0 {
            speed = total as f64 * 8.0 / 1e6 / elapsed;
        }
        if elapsed >= seconds {
            break;
        }
    }
    if speed <= 0.0 {
        return Err("download test produced no data".into());
    }
    Ok((speed * 10.0).round() / 10.0)
}

fn upload(agent: &ureq::Agent, seconds: f64) -> Result<f64, String> {
    let mut speed = 0.0f64;
    let blob = vec![0x42u8; 1_000_000];
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < deadline {
        let start = Instant::now();
        match agent.post(UP_URL).send_bytes(&blob) {
            Ok(_) => {
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    speed = blob.len() as f64 * 8.0 / 1e6 / elapsed;
                }
            }
            Err(e) => {
                if speed <= 0.0 {
                    return Err(format!("upload failed ({e})"));
                }
                break;
            }
        }
    }
    if speed <= 0.0 {
        return Err("upload test produced no data".into());
    }
    Ok((speed * 10.0).round() / 10.0)
}

/// Full test: ping, download, upload. Never panics; errors are reported
/// in `SpeedResult::error`.
pub fn run() -> SpeedResult {
    let agent = agent();
    let mut r = SpeedResult::default();
    match ping(&agent) {
        Ok(p) => r.ping_ms = p,
        Err(e) => {
            r.error = Some(e);
            return r;
        }
    }
    match download(&agent, 5.0) {
        Ok(d) => r.down_mbps = d,
        Err(e) => {
            r.error = Some(e);
            return r;
        }
    }
    match upload(&agent, 5.0) {
        Ok(u) => r.up_mbps = u,
        Err(e) => r.error = Some(e),
    }
    r
}
