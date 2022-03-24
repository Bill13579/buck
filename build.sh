#!/bin/bash
cargo build
mv target/armv7-unknown-linux-musleabihf/debug/buck buck
mv target/armv7-unknown-linux-musleabihf/debug/buck-cli buck