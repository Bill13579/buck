use std::env;
use std::os::unix::net::UnixStream;
use std::net::Shutdown;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::thread::sleep;
use std::time::Duration;

use std::fs::{File, OpenOptions};
use std::io::prelude::*;
pub fn root(s: &str) -> PathBuf {
    env::current_exe().unwrap().parent().unwrap().join(s)
}
fn log(s: &str) -> std::io::Result<()> {
    let mut opt = OpenOptions::new();
    let mut file = opt.write(true).read(false).append(true).create(true).open(root("log.txt"))?;
    file.write_all(format!("{}\n", s).as_bytes())?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    log("[cli] loading...");
    run(env::args().collect());
    Ok(())
}

fn run(args: Vec<String>) -> std::io::Result<()> {
    log(&format!("[cli] args {:?}", &args));
    log("[cli] attempting to connect to socket...");
    match UnixStream::connect("/tmp/buck.sock") {
        Ok(mut stream) => {
            log("[cli] success!");
            match args.len() {
                1 => {
                    stream.write_all(b"ui")?;
                    stream.flush();
                    stream.shutdown(Shutdown::Both);
                },
                _ => {
                    stream.write_all(b"select")?;
                    stream.flush();
                    stream.shutdown(Shutdown::Both);
                }
            }
        },
        Err(e) => {
            log(&format!("[cli] error: {:?}", &e));
            log("[cli] starting executable to compensate...");
            let result = Command::new(root("buck")).spawn();
            if let Err(e) = result {
                log(&format!("[cli] error: {:?}", &e));
                exit(1);
            } else if let Ok(c) = result {
                log("[cli] spawned");
            }
            log("[cli] going into loop to wait for executable to launch...");
            while let Err(_) = UnixStream::connect("/tmp/buck.sock") {
                sleep(Duration::from_millis(100));
            }
            return run(args);
        }
    }

    Ok(())
}