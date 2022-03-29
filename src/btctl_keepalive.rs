// btctl_keepalive.rs
// Prevent Bluetooth from turning itself off during playback (depends on bluetoothctl being available and on PATH)

use std::process::{Command, Stdio, Child, ChildStdin};
use std::thread::{self, sleep};
use std::sync::mpsc::{self, SyncSender, Receiver};

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Write, BufReader, Read};
use std::process::{exit};
use std::time::Duration;
use crate::read_config::root;
use crate::utils::elapsed::Elapsed;
use crate::{log, error, result};

pub struct BTKeepAlive {
    tx: SyncSender<usize>
}
impl BTKeepAlive {
    pub fn spawn() -> BTKeepAlive {
        let mut child = result!(Command::new("bluetoothctl")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn());
        let mut stdin = child.stdin.take().expect("no stdin on bluetoothctl");
        let (tx, rx): (SyncSender<usize>, Receiver<usize>) = mpsc::sync_channel(0);
        thread::spawn(move || {
            loop {
                let check_for_stop_signal = |wait_secs: u64| -> usize {
                    match rx.recv_timeout(Duration::from_secs(wait_secs)) {
                        Ok(v) => v,
                        Err(_) => 0
                    }
                };
                let solve_signal = |sig: usize, stdin: &mut ChildStdin| -> bool {
                    match sig {
                        1 => true,
                        2 => {
                            stdin.write_all(b"scan on\n");
                            let elapsed = Elapsed::new();
                            while elapsed.elapsed() <= Duration::from_secs(10) {
                                rx.try_recv();
                            }
                            false
                        },
                        _ => false
                    }
                };
                stdin.write_all(b"scan on\n");
                if solve_signal(check_for_stop_signal(20), &mut stdin) { break; }
                stdin.write_all(b"scan off\n");
                if solve_signal(check_for_stop_signal(10), &mut stdin) { break; }
                stdin.write_all(b"paired-devices\n");
                if solve_signal(check_for_stop_signal(10), &mut stdin) { break; }
                stdin.write_all(b"paired-devices\n");
                if solve_signal(check_for_stop_signal(10), &mut stdin) { break; }
            }
            child.kill();
        });
        BTKeepAlive { tx }
    }
    pub fn scan_on_temp(&mut self) {
        self.tx.send(2);
        sleep(Duration::from_secs(1));
    }
}
impl Drop for BTKeepAlive {
    fn drop(&mut self) {
        self.tx.send(1);
    }
}

