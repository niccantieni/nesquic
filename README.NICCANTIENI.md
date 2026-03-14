uv venv -p 3.14

source .venv/bin/activate
docker compose -f docker/backend.yml up -d



sudo apt install dnsmasq-base
sudo apt install protobuf-compiler
apt install ssl-cert
libssl-dev
binutils-dev
libpcap-dev

git clone git clone --recurse-submodules https://github.com/libbpf/bpftool.git
cd src
make
bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h
mv vmlinux.h ../nesquic/include/


curl -LO https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.0/clang+llvm-22.1.0-armv7a-linux-gnueabihf.tar.gz
tar xvf clang+llvm-22.1.0-armv7a-linux-gnueabihf.tar.gz
mv clang+llvm-22.1.0-armv7a-linux-gnueabihf llvm_build
LLVM_CONFIG=../../llvm_build/bin/llvm-config EXTRA_LDFLAGS=-static make -j -C src




git show e7ccc253227c1d744a5ce23ee388ba4a1258ee02 | git apply -R

export OUT_DIR=/home/ubuntu/nesquic/out

## after start
dockerd-rootless-setuptool.sh install
