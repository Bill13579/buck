#!/bin/bash

cd $(dirname "$0") && sudo mkdir -p build
cd build

. ../build-common.sh

heading build-deps.sh

log "==> Installing APT's ARM cross-compiling toolchain"
sudo apt-get -y install gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf binutils-arm-linux-gnueabihf
export APT_CC="arm-linux-gnueabihf-gcc"

log "==> Downloading additional toolchains (1/1 Linaro 2017.01)"
if [ ! -d "gcc-linaro-4.9.4-2017.01-x86_64_arm-linux-gnueabihf" ]; then
    sudo wget -nc "https://releases.linaro.org/components/toolchain/binaries/4.9-2017.01/arm-linux-gnueabihf/gcc-linaro-4.9.4-2017.01-x86_64_arm-linux-gnueabihf.tar.xz"
    sudo tar --keep-old-files -xvf "gcc-linaro-4.9.4-2017.01-x86_64_arm-linux-gnueabihf.tar.xz"
fi
export LINARO_CC="/vagrant/build/gcc-linaro-4.9.4-2017.01-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc"

# # Build zlib
# log "==> Building zlib"
# sudo git clone https://github.com/madler/zlib.git
# cd zlib
# export CC=${APT_CC}
# sudo ./configure --prefix=/vagrant/build/zlib
# sudo make
# sudo make install
# cd ..

# # Build alsa-lib
# log "==> Building alsa-lib"
# sudo apt-get install -y libtool
# sudo git clone https://github.com/alsa-project/alsa-lib.git
# cd alsa-lib
# export CC=${LINARO_CC}
# sudo libtoolize
# sudo aclocal
# sudo autoheader
# env CC="gcc -m32" sudo autoconf
# sudo ./configure \
#         --host=arm-linux-gnueabihf \
#         CC=arm-linux-gnueabihf-gcc \
#         --enable-shared \
#         --disable-python \
#         --prefix=/usr/local/mplayer
# env CC="gcc -m32" sudo automake --add-missing
# env CC="gcc -m32" sudo make
# env CC="gcc -m32" sudo make install
# cd ..

# # Build mplayer
# log "==> Building mplayer"
# sudo svn checkout svn://svn.mplayerhq.hu/mplayer/trunk mplayer
# export CC=${LINARO_CC}
# cd mplayer
# sudo ./configure --enable-cross-compile --prefix=/usr/local/mplayer \
#             --enable-runtime-cpudetection \
#             --cc=${LINARO_CC} \
#             --target=arm-armv7-linux \
#             --prefix=./build \
#             --enable-alsa \
#             --extra-cflags="-I/vagrant/build/zlib/include -I/usr/local/mplayer/include/" \
#             --extra-ldflags="-L/vagrant/build/zlib/lib -L/usr/local/mplayer/lib -lasound" \
#             --enable-ass \
#             --host-cc=gcc \
#             --enable-fbdev --disable-dvdread \
#             --disable-dvdnav --disable-jpeg --disable-tga \
#             --disable-pnm --disable-tv \
#             --disable-xanim --disable-win32dll --disable-armv5te --disable-armv6 \
#             --disable-png  2>&1 | sudo tee logfile
# sudo make
# cd ..

# # Collect build artifacts for mplayer, zlib, and alsa-lib
# log "==> Collecting build artifacts for mplayer, zlib, and alsa-lib"
# sudo mkdir -p artifacts
# sudo mkdir -p artifacts/lib/
# sudo mkdir -p artifacts/usr/lib/
# sudo mkdir -p artifacts/usr/local/
# sudo cp /vagrant/build/zlib/lib/ -a ./artifacts/usr/
# sudo rm -rf ./artifacts/usr/lib/pkgconfig ./artifacts/usr/lib/libz.a
# sudo cp /vagrant/build/mplayer/mplayer ./artifacts/
# sudo cp /usr/local/mplayer/lib/libasound.so.2.0.0 ./artifacts/lib/
# cd ./artifacts/lib/ && sudo ln -s ./libasound.so.2.0.0 ./libasound.so.2 && cd ../../
# sudo cp /usr/local/mplayer/* -a ./artifacts/usr/local/

log "==> Building canvas"
export CC=${LINARO_CC}
cd ../canvas
sudo rustup target add armv7-unknown-linux-gnueabihf
sudo cargo build --release

#, spotifyd

