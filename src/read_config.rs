use std::{env, path::PathBuf};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Write, BufReader, Read};
use std::process::{exit};

use crate::{log, error, result};

use serde::{Serialize, Deserialize};
use serde_json::Result;

#[derive(Serialize, Deserialize)]
pub struct BuckConfig {
    pub music_dirs: Vec<String>
}

pub fn read_config() -> BuckConfig {
    let config_path = result!(env::current_exe()).parent().unwrap().join("config.json");
    let mut config_file = result!(OpenOptions::new().write(false).read(true).create(false).open(config_path));
    let mut config_str = String::new();
    config_file.read_to_string(&mut config_str);
    result!(serde_json::from_str(&config_str))
}