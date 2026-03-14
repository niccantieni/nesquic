# nesquic: Nic's personal README

## Additional Dependencies
sudo apt install dnsmasq-base protobuf-compiler ssl-cert libssl-dev binutils-dev libpcap-dev 

## Generate `linuxvm.h`

```bash

git clone git clone --recurse-submodules https://github.com/libbpf/bpftool.git

# maybe not here, but later, unsure
cd src
make

# install LLVM
curl -LO https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.0/clang+llvm-22.1.0-armv7a-linux-gnueabihf.tar.gz
tar xvf clang+llvm-22.1.0-armv7a-linux-gnueabihf.tar.gz
mv clang+llvm-22.1.0-armv7a-linux-gnueabihf llvm_build

cd bpftool
LLVM_CONFIG=../../llvm_build/bin/llvm-config EXTRA_LDFLAGS=-static make -j -C src

bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h
mv vmlinux.h ../nesquic/include/
```


export OUT_DIR=/home/ubuntu/nesquic/out

## after start
dockerd-rootless-setuptool.sh install
