#!/bin/bash

pkill -f buck
pkill -f buck-cli
pkill -f mplayer
rm -rf "/mnt/us/documents/Buck - Table of Contents.pdf" ||:
rm -rf "/tmp/buck.sock" ||:
exec /mnt/us/buck/buck-cli
return 0
