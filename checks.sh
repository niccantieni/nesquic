echo neqo
cargo check -p nesquic --features neqo
echo quinn
cargo check -p nesquic --features quinn
echo noq
cargo check -p nesquic --features noq
echo quiche
cargo check -p nesquic --features quiche