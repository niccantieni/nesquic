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

apt install -y build-essential git uidmap pkgconf zlib1g-dev libelf-dev clang libnuma-dev dnsmasq-base protobuf-compiler ssl-cert libssl-dev binutils-dev libpcap-dev apache2-bin apache2-dev cmake libcairo2-dev libpango1.0-dev libxcb-present-dev libnss3-dev clang-18
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

## install NSS and NSPR
follow https://github.com/mozilla/neqo

I copied the dist folder to static_dependencies/neqo_dependencies/dist and created empty folder nss and nspr inthere (just for the environment variables to work)


export LD_LIBRARY_PATH=/home/ubuntu/nesquic/static_dependencies/neqo_dependencies/dist/Release/lib
export NSS_DIR=/home/ubuntu/nesquic/static_dependencies/neqo_dependencies/nss
export NSS_PREBUILT=1


## VM
sudo ufw allow 8080
redir -n :8080 10.14.80.40:8080


set AllowTcpForwarding yes before Include... in /etc/ssh/sshd_config
sudo systemctl restart ssh

chmod a+r -R docker
