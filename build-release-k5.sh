#!/bin/bash
cargo build --release --features=kindle
mv target/armv7-unknown-linux-musleabihf/release/buck buck
mv target/armv7-unknown-linux-musleabihf/release/buck-cli buck
./k5-packager.sh