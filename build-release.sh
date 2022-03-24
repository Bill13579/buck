#!/bin/bash
cargo build --release
mv target/armv7-unknown-linux-musleabihf/release/buck buck
mv target/armv7-unknown-linux-musleabihf/release/buck-cli buck