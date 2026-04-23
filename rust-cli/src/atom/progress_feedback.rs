use std::time::Duration;

const DEFAULT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_SPINNER_INTERVAL: Duration = Duration::from_millis(100);
const SPINNER_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];

pub(crate) fn heartbeat_interval() -> Duration {
    std::env::var("STATA_CLI_PROGRESS_INTERVAL_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL)
}

pub(crate) fn spinner_interval() -> Duration {
    DEFAULT_SPINNER_INTERVAL
}

pub(crate) fn backend_heartbeat_message(elapsed: Duration) -> String {
    format!("stata-cli: still running... elapsed={}s", elapsed.as_secs())
}

pub(crate) fn spinner_message(elapsed: Duration, frame_index: usize, colorize: bool) -> String {
    let frame = SPINNER_FRAMES[frame_index % SPINNER_FRAMES.len()];
    let message = format!("{frame} running Stata... {}s", elapsed.as_secs());
    if colorize {
        format!("\x1b[90m{message}\x1b[0m")
    } else {
        message
    }
}

pub(crate) fn clear_terminal_line() -> &'static str {
    "\r\x1b[2K"
}

pub(crate) fn prompt_status_line(
    prefix: &str,
    line: &str,
    success: bool,
    colorize: bool,
) -> String {
    if !colorize {
        return format!("{prefix} {line}");
    }
    let color = if success { "\x1b[1;32m" } else { "\x1b[1;31m" };
    format!("{color}{prefix}\x1b[0m {line}")
}

#[cfg(test)]
mod tests {
    use super::{backend_heartbeat_message, prompt_status_line, spinner_message};
    use std::time::Duration;

    #[test]
    fn backend_heartbeat_message_reports_elapsed_seconds() {
        assert_eq!(
            backend_heartbeat_message(Duration::from_secs(31)),
            "stata-cli: still running... elapsed=31s"
        );
    }

    #[test]
    fn spinner_message_can_render_plain_and_colored_text() {
        assert_eq!(
            spinner_message(Duration::from_secs(12), 0, false),
            "- running Stata... 12s"
        );
        assert!(spinner_message(Duration::from_secs(12), 1, true).contains("\x1b[90m"));
    }

    #[test]
    fn prompt_status_line_colors_success_and_failure_prefixes() {
        assert_eq!(
            prompt_status_line(".", "clear all", true, false),
            ". clear all"
        );
        assert!(prompt_status_line(".", "clear all", true, true).starts_with("\x1b[1;32m."));
        assert!(prompt_status_line(".", "bad", false, true).starts_with("\x1b[1;31m."));
    }
}
