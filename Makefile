.PHONY: clippy-fixes clippy-strict

all: clippy-strict

clippy-fixes:
	cargo clippy --fix -- -W clippy::pedantic -W clippy::nursery -W clippy::unwrap_used -W clippy::expect_used

clippy-strict:
	cargo clippy -- -W clippy::pedantic -W clippy::nursery -W clippy::unwrap_used -W clippy::expect_used

