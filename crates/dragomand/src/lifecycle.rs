// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon lifecycle: the keep-warm sweeper, memory-pressure watching, and
//! idle exit. A daemon that sits around consuming resources permanently is
//! a design failure; this module makes it go away.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::service::DaemonState;

/// Tracks when the daemon last did anything for a client.
pub struct Activity {
    last: Mutex<Instant>,
}

impl Default for Activity {
    fn default() -> Self {
        Activity {
            last: Mutex::new(Instant::now()),
        }
    }
}

impl Activity {
    pub fn touch(&self) {
        *self.last.lock().expect("activity lock") = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        self.last.lock().expect("activity lock").elapsed()
    }
}

/// Runs the keep-warm policy until the daemon shuts down.
pub async fn run_sweeper(state: Arc<DaemonState>) {
    let window = state.config.keep_warm_window();
    let interval = window
        .div_f32(4.0)
        .clamp(Duration::from_millis(250), Duration::from_secs(30));
    loop {
        tokio::time::sleep(interval).await;
        state.workers.sweep(state.config.keep_warm, window).await;
    }
}

/// Resolves when the daemon has been idle long enough to exit: no
/// in-flight requests, nothing loaded (the sweeper unloads warm models
/// first) and no activity for `idle_exit`.
pub async fn wait_until_idle(state: Arc<DaemonState>) {
    let idle_exit = state.config.idle_exit();
    let poll = idle_exit
        .div_f32(4.0)
        .clamp(Duration::from_millis(100), Duration::from_secs(10));
    loop {
        tokio::time::sleep(poll).await;
        if state.requests.active_count() == 0
            && state.workers.is_empty().await
            && state.activity.idle_for() >= idle_exit
        {
            return;
        }
    }
}

/// Watches for memory pressure and evicts everything unpinned when it
/// hits. Uses the systemd memory-pressure protocol when
/// `$MEMORY_PRESSURE_WATCH` is set, a PSI trigger on
/// `/proc/pressure/memory` otherwise. Never fails the daemon: systems
/// without PSI simply get no pressure handling.
///
/// The blocking poll lives on a plain `std::thread` holding only a `Weak`
/// on the state, so it never keeps a tokio runtime (or the daemon's tests)
/// from shutting down; it notices the daemon is gone within one poll
/// timeout and exits.
pub async fn watch_memory_pressure(state: Arc<DaemonState>) {
    // https://systemd.io/MEMORY_PRESSURE/
    let (path, trigger) = match std::env::var("MEMORY_PRESSURE_WATCH") {
        Ok(path) if path == "/dev/null" => {
            tracing::debug!("memory pressure watching disabled by MEMORY_PRESSURE_WATCH");
            return;
        }
        Ok(path) => {
            let trigger = std::env::var("MEMORY_PRESSURE_WRITE")
                .ok()
                .and_then(|b64| base64_decode(&b64))
                .unwrap_or_else(default_trigger);
            (path, trigger)
        }
        Err(_) => ("/proc/pressure/memory".to_owned(), default_trigger()),
    };

    let fired = Arc::new(tokio::sync::Notify::new());
    let weak = Arc::downgrade(&state);
    {
        let fired = Arc::clone(&fired);
        std::thread::Builder::new()
            .name("dragoman-psi".into())
            .spawn(
                move || match poll_pressure_loop(&path, &trigger, &weak, &fired) {
                    Ok(()) => {}
                    Err(reason) => tracing::debug!(reason, "memory pressure watching unavailable"),
                },
            )
            .expect("spawning the PSI thread");
    }

    loop {
        fired.notified().await;
        tracing::warn!("memory pressure; unloading idle models");
        state.workers.evict_unpinned().await;
        // Debounce: PSI triggers fire at most once per window anyway.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// PSI trigger: at least 150ms of "some" memory stall within 1s.
fn default_trigger() -> Vec<u8> {
    b"some 150000 1000000".to_vec()
}

/// Polls the PSI file until the owning daemon goes away.
fn poll_pressure_loop(
    path: &str,
    trigger: &[u8],
    daemon: &std::sync::Weak<DaemonState>,
    fired: &tokio::sync::Notify,
) -> Result<(), String> {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("{path}: {e}"))?;
    file.write_all(trigger)
        .map_err(|e| format!("{path}: {e}"))?;

    loop {
        let mut fds = [rustix::event::PollFd::new(
            &file,
            rustix::event::PollFlags::PRI,
        )];
        let n = rustix::event::poll(&mut fds, Some(&Duration::from_secs(2).try_into().unwrap()))
            .map_err(|e| format!("poll {path}: {e}"))?;
        if daemon.strong_count() == 0 {
            return Ok(());
        }
        if n == 0 {
            continue; // Timeout: just re-check liveness.
        }
        if fds[0].revents().contains(rustix::event::PollFlags::ERR) {
            return Err(format!("{path}: poll reported an error"));
        }
        fired.notify_one();
    }
}

/// Minimal base64 (standard alphabet, padding optional): enough for
/// systemd's MEMORY_PRESSURE_WRITE.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for c in input.bytes() {
        if c == b'=' || c == b'\n' {
            continue;
        }
        let value = ALPHABET.iter().position(|&a| a == c)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_roundtrip() {
        assert_eq!(
            super::base64_decode("c29tZSAxNTAwMDAgMjAwMDAwMA==").as_deref(),
            Some(&b"some 150000 2000000"[..])
        );
        assert_eq!(super::base64_decode("!!!"), None);
    }
}
