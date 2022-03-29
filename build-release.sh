#!/bin/bash
cargo build --release --features=btonly,kobo
mv target/armv7-unknown-linux-musleabihf/release/buck buck
mv target/armv7-unknown-linux-musleabihf/release/buck-cli buck