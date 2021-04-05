build:
	cargo build-bpf

unit:
	cargo test --no-fail-fast --lib

integration:
	cargo test-bpf --test lib

test: unit integration