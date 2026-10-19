//! HANCORE external process isolation module for OmaProton.
//! Spawns child processes in isolated process groups (`process_group(0)`),
//! enforces RAII `ProcessGroupGuard` cleanup (`kill(-pid, SIGTERM/SIGKILL)`),
//! and bounds stdout reads with non-blocking `poll()`.

use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

const POLLIN: i16 = 0x0001;
const POLLHUP: i16 = 0x0010;
const POLLERR: i16 = 0x0008;

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 2048;

extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
}

pub fn reap_process_group(child: &mut std::process::Child, pid: i32) {
    if pid > 1 {
        // SAFETY: Sending SIGTERM/SIGKILL to negative PID targets the isolated process group.
        unsafe {
            kill(-pid, 15);
        }
        std::thread::sleep(Duration::from_millis(5));
        unsafe {
            kill(-pid, 9);
        }
    }
    let _ = child.wait();
}

pub struct ProcessGroupGuard<'a> {
    pub child: &'a mut std::process::Child,
    pub pid: i32,
    pub active: bool,
}

impl<'a> Drop for ProcessGroupGuard<'a> {
    fn drop(&mut self) {
        if self.active {
            reap_process_group(self.child, self.pid);
        }
    }
}

/// Validates that an argument contains only safe characters (no shell metacharacters or control bytes).
pub fn is_safe_arg(arg: &str) -> bool {
    !arg.is_empty()
        && arg.len() <= 256
        && arg.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '+')
        })
}

/// Runs an external binary in an isolated process group (`process_group(0)`) with bounded output.
pub fn run_cmd_bounded(
    cmd_path: &str,
    args: &[&str],
    deadline: Instant,
    max_output_bytes: usize,
) -> Option<String> {
    if Instant::now() >= deadline {
        return None;
    }
    if !args.iter().all(|a| is_safe_arg(a)) {
        return None;
    }

    let mut cmd = Command::new(cmd_path);
    cmd.args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    cmd.process_group(0);

    let mut child = cmd.spawn().ok()?;
    let pid = child.id() as i32;
    let mut stdout = child.stdout.take()?;
    let raw_fd = stdout.as_raw_fd();

    // SAFETY: Standard POSIX fcntl on a valid pipe descriptor.
    unsafe {
        let flags = fcntl(raw_fd, F_GETFL, 0);
        if flags >= 0 {
            let _ = fcntl(raw_fd, F_SETFL, flags | O_NONBLOCK);
        }
    }

    let mut guard = ProcessGroupGuard {
        child: &mut child,
        pid,
        active: true,
    };

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut stdout_closed = false;
    let mut child_exited = false;
    let mut child_ok = false;
    let mut overrun = false;
    let mut failed = false;

    loop {
        if Instant::now() >= deadline {
            break;
        }

        if !child_exited {
            match guard.child.try_wait() {
                Ok(Some(status)) => {
                    child_exited = true;
                    child_ok = status.success();
                }
                Ok(None) => {}
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }

        if child_exited && stdout_closed {
            break;
        }

        if stdout_closed {
            let rem = deadline.saturating_duration_since(Instant::now());
            if rem.is_zero() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5).min(rem));
            continue;
        }

        let rem_ms = (deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(50) as i32)
            .max(1);
        let mut pfd = PollFd {
            fd: raw_fd,
            events: POLLIN | POLLHUP | POLLERR,
            revents: 0,
        };

        // SAFETY: Valid single PollFd pointer.
        let ret = unsafe { poll(&mut pfd, 1, rem_ms) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            failed = true;
            break;
        } else if ret == 0 {
            continue;
        }

        if pfd.revents & (POLLIN | POLLHUP | POLLERR) != 0 {
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => {
                        stdout_closed = true;
                        break;
                    }
                    Ok(n) => {
                        if buffer.len() + n > max_output_bytes {
                            overrun = true;
                            break;
                        }
                        buffer.extend_from_slice(&chunk[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        stdout_closed = true;
                        break;
                    }
                }
            }
            if pfd.revents & (POLLHUP | POLLERR) != 0 {
                stdout_closed = true;
            }
            if overrun || failed {
                break;
            }
        }
    }

    if overrun || failed || !child_ok || Instant::now() >= deadline {
        return None;
    }

    guard.active = false;
    reap_process_group(guard.child, guard.pid);
    String::from_utf8(buffer).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_group_timeout() {
        let deadline = Instant::now() + Duration::from_millis(80);
        let res = run_cmd_bounded("/bin/sleep", &["--", "3"], deadline, 1024);
        assert!(res.is_none());
    }

    #[test]
    fn test_arg_validation_blocks_injection() {
        let deadline = Instant::now() + Duration::from_millis(500);
        assert!(!is_safe_arg("; rm -rf /"));
        assert!(run_cmd_bounded("/bin/echo", &["; rm -rf /"], deadline, 1024).is_none());
    }
}
