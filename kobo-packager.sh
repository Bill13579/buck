#!/bin/bash

mkdir kobo-packaging
cd kobo-packaging
mkdir -p ./mnt/onboard
mkdir -p ./mnt/onboard/.adds/nm
mkdir -p ./mnt/onboard/.buck
mkdir -p ./mnt/onboard/.buck/bin
mkdir -p ./mnt/onboard/.buck/assets
mkdir -p ./mnt/onboard/buck
mkdir -p ./mnt/onboard/music
cp ../buck/buck ./mnt/onboard/.buck
cp ../buck/buck-cli ./mnt/onboard/.buck
cp ../buck/config-sample-kobo.json ./mnt/onboard/.buck/config.json
cp -a ../buck/assets/. ./mnt/onboard/.buck/assets/
cp ../buck/bin/mplayer-armhf ./mnt/onboard/.buck/bin/mplayer
cp ../buck/buck.nmconfig ./mnt/onboard/.adds/nm
cp ../README.md ./mnt/onboard/.buck/
cp ../LICENSE ./mnt/onboard/.buck/
wget https://raw.githubusercontent.com/mathiasbynens/small/master/pdf.pdf
mv pdf.pdf "./mnt/onboard/buck/Buck - Table of Contents.pdf"
cd ..
tar -czvf KoboRoot.tgz -C kobo-packaging .