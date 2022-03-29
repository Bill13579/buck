#!/bin/bash

# Vagrant provisioner script

cd $(dirname "$0") && mkdir -p build
cd build

export LC_CTYPE=C.UTF-8
export DEBIAN_FRONTEND=noninteractive

echo "  [build-deps-pre] Running apt-get update/upgrade"
sudo apt-get update && sudo apt-get upgrade -y && \
  sudo apt-get install -y apt-utils software-properties-common && \
  sudo apt-add-repository universe && \
  sudo apt-get update

echo "  [build-deps-pre] Installing build tools"
sudo apt-get install -y build-essential gcc gcc-multilib make bsdmainutils autotools-dev automake patchelf libc-bin \
  gdb gdb-multiarch strace ltrace \
  wget curl netcat git subversion \
  vim

echo "  [build-deps-pre] Installing 32-bit binary support"
sudo dpkg --add-architecture i386 && \
  sudo apt-get update && \
  sudo apt-get install -y libc6:i386 libncurses5:i386 libstdc++6:i386

echo "  [build-deps-pre] Installing Rustlang"
export HOME="/home/vagrant"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

echo "  [build-deps-pre] Installing kernel build dependencies"
sudo apt-get install -y git fakeroot build-essential ncurses-dev xz-utils libssl-dev bc flex libelf-dev bison

echo "  [build-deps-pre] BUILD ENVIRONMENT READY, EXITING"

