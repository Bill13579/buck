#!/bin/bash
cargo build --features=btonly,kobo
mv target/armv7-unknown-linux-musleabihf/debug/buck buck
mv target/armv7-unknown-linux-musleabihf/debug/buck-cli buck
./kobo-packager.sh