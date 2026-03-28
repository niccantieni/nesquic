# nesquic: Nic's personal README

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
apt update

apt install -y build-essential git uidmap pkgconf zlib1g-dev libelf-dev clang-18 libnuma-dev dnsmasq-base protobuf-compiler ssl-cert libssl-dev binutils-dev libpcap-dev apache2-bin apache2-dev cmake libcairo2-dev libpango1.0-dev libxcb-present-dev libnss3-dev
sudo rm -f /bin/clang && sudo ln -s /usr/bin/clang-18 /bin/clang
curl -fsSL https://get.docker.com -o get-docker.sh && sudo sh ./get-docker.sh
dockerd-rootless-setuptool.sh install
usermod -aG docker ubuntu
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y


sudo multipass transfer --recursive --parents . XXXX:home/ubuntu/nesquic¨


### .bashrc in guest vm
export PATH="/home/ubuntu/.local/bin:$PATH"

setopt appendhistory
setopt INC_APPEND_HISTORY
setopt SHARE_HISTORY

## NSS Database setup
sudo apt isntall libnss3-tools
certutil -N -d res/nssdb
openssl pkcs12 -export -in res/pem/cert.pem -inkey res/pem/key.pem -out res/pkcs12/cert.p12 -name "nesquic"
certutil -N -d res/nssdb
pk12util -i res/pkcs12/cert.p12 -d res/nssdb
certutil -L -d nssdb
certutil -M -t "TC,C,C" -n nesquic -d nssdb
certutil -L -d nssdb

## NSS patch all libraries
sudo apt install patchelf
strace -e trace=openat target/release/nesquic-neqo
export LIBDIR=/home/ubuntu/nesquic/static_dependencies/neqo_dependencies/dist/Release/lib
find $LIBDIR -name "*.so*" -type f | xargs -I{} patchelf --set-rpath $LIBDIR {}
readelf -d $LIBDIR/libnss3.so | grep -E "RPATH|RUNPATH"

## NSS patch binary ATTN AFTER EVERY RUN
sudo patchelf --set-rpath /home/ubuntu/nesquic/static_dependencies/neqo_dependencies/dist/Release/lib target/release/nesquic-neqo


echo "/home/ubuntu/nesquic/static_dependencies/neqo_dependencies/dist/Release/lib" | sudo tee /etc/ld.so.conf.d/nesquic.conf
sudo ldconfig
ldconfig -p

maybe?
```
Cargo.toml
# rpath = false

[profile.dev]
# rpath = true
```



## VM
sudo ufw allow 8080
redir -n :8080 10.14.80.40:8080


set AllowTcpForwarding yes before Include... in /etc/ssh/sshd_config
sudo systemctl restart ssh

chmod a+r -R docker
