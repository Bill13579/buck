#!/bin/bash

mkdir -p buck-k5

cp -a ./buck/. ./buck-k5/
rm -rf ./buck-k5/bin/mplayer-armhf
mv ./buck-k5/bin/mplayer-arm ./buck-k5/bin/mplayer
rm ./buck-k5/buck.nmconfig
rm ./buck-k5/config-sample-kobo.json
mv ./buck-k5/config-sample-kindle.json ./buck-k5/config.json