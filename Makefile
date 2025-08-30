.PHONY: all test clippy-fixes clippy-strict cache-test build clean

all: test clippy-strict build

build:
	cargo build --release

test:
	cargo test --lib
	cargo test --test cache_integration_test

cache-test:
	cargo test --lib k8s::cache
	cargo test --test cache_integration_test

clippy-fixes:
	cargo clippy --fix -- -W clippy::pedantic -W clippy::nursery -W clippy::unwrap_used -W clippy::expect_used

clippy-strict:
	cargo clippy -- -W clippy::pedantic -W clippy::nursery -W clippy::unwrap_used -W clippy::expect_used

clean:
	cargo clean

