BINARY := sigmabridge
RELEASE_DIR := target/release

.PHONY: all build run release start check test fmt clean size

all: build

build:
	cargo build

run:
	cargo run

release:
	cargo build --release

start: release
	./$(RELEASE_DIR)/$(BINARY)

check:
	cargo check

test:
	cargo test

fmt:
	cargo fmt

clean:
	cargo clean

size: release
	@ls -lh $(RELEASE_DIR)/$(BINARY) | awk '{print "Binary size:", $$5}'
