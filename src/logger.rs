// logger.rs
// Logger

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Write, BufReader, Read};
use std::process::{exit};
use crate::read_config::root;

#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! log {
    ($src:expr, $x:expr) => {};
    ($src:expr, $x:expr, $($y:expr),+) => {}
}

#[macro_export]
#[cfg(debug_assertions)]
macro_rules! log {
    ($src:expr, $x:expr) => {
        let mut opt = OpenOptions::new();
        let mut file = opt.write(true).read(false).append(true).create(true).open(root("log.txt")).unwrap();
        file.write_all(format!("[{}] {}\n", $src, $x).as_bytes());
        file.flush();
    };
    ($src:expr, $x:expr, $($y:expr),+) => {
        log!($src, format!($x, $($y),+));
    }
}


#[macro_export]
macro_rules! error {
    ($src:expr, $x:expr) => {
        let mut opt = OpenOptions::new();
        let mut file = opt.write(true).read(false).append(true).create(true).open(root("log.txt")).unwrap();
        file.write_all(format!("[ERROR {}] {}\n", $src, $x).as_bytes());
        file.flush();
    };
    ($src:expr, $x:expr, $($y:expr),+) => {
        error!($src, format!($x, $($y),+));
    }
}

#[macro_export]
macro_rules! result {
    ($x:expr) => {
        match $x {
            Ok(r) => r,
            Err(e) => {
                error!("fatal", "{:?}", e);
                std::process::exit(1)
            }
        }
    };
}

#[derive(Debug, Clone)]
struct TestError;
impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid first item to double")
    }
}

pub fn test() {
    log!("main", "test normal");
    log!("main", "test format {} {} {}", 1, 2, 3);
    error!("main", "test error");
    error!("main", "test error format {} {} {}", 1, 2, 3);
    let b: Result<String, TestError> = Ok(String::from("ok!"));
    log!("main", "test result success: {}", result!(b));
    let a: Result<String, TestError> = Err(TestError);
    log!("main", "test result success: {}", result!(a));
}
