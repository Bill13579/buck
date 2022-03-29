#!/bin/bash
cargo build --features=kindle
mv target/armv7-unknown-linux-musleabihf/debug/buck buck
mv target/armv7-unknown-linux-musleabihf/debug/buck-cli buck
./k5-packager.sh