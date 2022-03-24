use std::{process::{Command, Output, ExitStatus}, os::unix::prelude::CommandExt};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Write, BufReader, Read};
use std::process::{exit};
use crate::{result, log, error};

pub fn quick_run(target: &str, args: Vec<&str>) {
    let mut child = result!(Command::new(target).args(args).spawn());
    child.wait();
}

pub fn quick_write(l: usize, s: &str) {
    Command::new("fbink").args(vec!["-y", &(l+1).to_string(), "-Y", "100", "-C", "GRAY8", "-S", "2", s]).spawn();
}