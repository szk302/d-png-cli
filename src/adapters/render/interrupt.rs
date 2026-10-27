//! While a renderer runs, turns termination requests into an error at the next
//! wait, so the renderer's process group and temporary files are cleaned up as
//! on any other failure. Renderers run in their own process group and do not
//! receive the terminal's Ctrl+C themselves. Outside a renderer, signals keep
//! their default behavior.
use anyhow::{Result, bail};
use std::sync::{
    Mutex,
    atomic::{AtomicI32, Ordering},
};

static SIGNAL: AtomicI32 = AtomicI32::new(0);

#[cfg(unix)]
const SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

struct State {
    depth: usize,
    #[cfg(unix)]
    previous: Vec<(libc::c_int, libc::sigaction)>,
}
// SAFETY: the saved dispositions are plain data, only used under the mutex.
unsafe impl Send for State {}

static STATE: Mutex<State> = Mutex::new(State {
    depth: 0,
    #[cfg(unix)]
    previous: Vec::new(),
});

/// Handles SIGINT, SIGTERM and SIGHUP (console Ctrl+C and Ctrl+Break on
/// Windows) until dropped. Declare it before the renderer's resources so it is
/// dropped after them: if a request arrived, it then restores the previous
/// handling and terminates the process with that signal, before any output is
/// saved. A second request terminates immediately. Signals that were ignored
/// stay ignored.
pub struct Guard(());

impl Guard {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if state.depth == 0 {
            SIGNAL.store(0, Ordering::Relaxed);
            install(&mut state);
        }
        state.depth += 1;
        Guard(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.depth -= 1;
        if state.depth > 0 {
            return;
        }
        restore(&mut state);
        drop(state);
        let signal = SIGNAL.swap(0, Ordering::Relaxed);
        if signal != 0 {
            terminate(signal);
        }
    }
}

/// Fails once a termination request has been received.
pub fn check() -> Result<()> {
    let signal = SIGNAL.load(Ordering::Relaxed);
    if signal != 0 {
        bail!("interrupted (signal {signal})");
    }
    Ok(())
}

#[cfg(unix)]
extern "C" fn record(signal: libc::c_int) {
    SIGNAL.store(signal, Ordering::Relaxed);
}

#[cfg(unix)]
fn install(state: &mut State) {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = record as extern "C" fn(libc::c_int) as usize;
        // Restart interrupted reads; waits poll for the signal at least every 100 ms.
        action.sa_flags = libc::SA_RESTART | libc::SA_RESETHAND;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in SIGNALS {
            let mut previous: libc::sigaction = std::mem::zeroed();
            libc::sigaction(signal, std::ptr::null(), &mut previous);
            // For example, nohup ignores SIGHUP and shells ignore SIGINT for
            // background jobs.
            if previous.sa_sigaction == libc::SIG_IGN {
                continue;
            }
            libc::sigaction(signal, &action, std::ptr::null_mut());
            state.previous.push((signal, previous));
        }
    }
}

#[cfg(unix)]
fn restore(state: &mut State) {
    for (signal, previous) in state.previous.drain(..) {
        unsafe { libc::sigaction(signal, &previous, std::ptr::null_mut()) };
    }
}

#[cfg(unix)]
fn terminate(signal: libc::c_int) -> ! {
    eprintln!("error: interrupted (signal {signal})");
    unsafe {
        // Terminate as the signal would have without a handler.
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
    std::process::exit(128 + signal);
}

#[cfg(windows)]
unsafe extern "system" fn record(event: u32) -> windows_sys::core::BOOL {
    use windows_sys::Win32::{
        Foundation::{FALSE, TRUE},
        System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT},
    };
    if (event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
        && SIGNAL.swap(2, Ordering::Relaxed) == 0
    {
        TRUE
    } else {
        FALSE
    }
}

#[cfg(windows)]
fn install(_: &mut State) {
    use windows_sys::Win32::{Foundation::TRUE, System::Console::SetConsoleCtrlHandler};
    unsafe { SetConsoleCtrlHandler(Some(record), TRUE) };
}

#[cfg(windows)]
fn restore(_: &mut State) {
    use windows_sys::Win32::{Foundation::FALSE, System::Console::SetConsoleCtrlHandler};
    unsafe { SetConsoleCtrlHandler(Some(record), FALSE) };
}

#[cfg(windows)]
fn terminate(_: i32) -> ! {
    eprintln!("error: interrupted");
    // STATUS_CONTROL_C_EXIT, as for a process ended by Ctrl+C.
    std::process::exit(0xC000013A_u32 as i32);
}
