//! Interruptible subprocesses. Each command owns a process group, including Cargo's children.
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub fn install_handler() -> Result<()> {
    ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::Relaxed))?;
    Ok(())
}
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}
pub fn check_interrupt() -> Result<()> {
    ensure!(
        !interrupted(),
        "benchmark interrupted; temporary worktrees cleaned up"
    );
    Ok(())
}

struct ChildGuard {
    child: Child,
    reaped: bool,
}
impl ChildGuard {
    fn terminate(&mut self) {
        // Kill the entire group BEFORE reaping its leader, keeping its PGID reserved.
        #[cfg(unix)]
        {
            use nix::{
                sys::signal::{Signal, killpg},
                unistd::Pid,
            };
            if let Ok(id) = i32::try_from(self.child.id()) {
                let _ = killpg(Pid::from_raw(id), Signal::SIGKILL);
            }
        }
        let _ = self.child.kill();
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.reaped {
            self.terminate();
            let _ = self.child.wait();
        }
    }
}

pub fn checked(command: &mut Command) -> Result<Output> {
    check_interrupt()?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut guard = ChildGuard {
        child: command
            .spawn()
            .with_context(|| format!("launching {command:?}"))?,
        reaped: false,
    };
    let child = &mut guard.child;
    let stdout = child.stdout.take().context("missing child stdout")?;
    let stderr = child.stderr.take().context("missing child stderr")?;
    // Drain both pipes while Cargo runs so a full pipe cannot deadlock cancellation.
    let out = thread::spawn(move || read_pipe(stdout));
    let err = thread::spawn(move || read_pipe(stderr));
    let status = loop {
        if interrupted() {
            guard.terminate();
            guard.child.wait()?;
            guard.reaped = true;
            // Do not block on inherited stdout held by an escaped descendant.
            // Reader threads are detached on this terminal error; main exits next.
            anyhow::bail!("benchmark interrupted; child process group terminated");
        }
        // Keep the leader unreaped (and its PGID reserved) while descendants can
        // still hold output pipes. Cancellation remains observable during draining.
        if out.is_finished()
            && err.is_finished()
            && let Some(status) = guard.child.try_wait()?
        {
            guard.reaped = true;
            break status;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = out
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
    ensure!(
        status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
fn read_pipe(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}
