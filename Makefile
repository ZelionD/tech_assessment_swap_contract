TARGET=wasm32-unknown-unknown
TARGETDIR = target/${TARGET}/release
RELEASEFLAGS = --target ${TARGET} --release

.PHONY: build
build:
	rustup target add ${TARGET}
	cargo build $(RELEASEFLAGS)

.PHONY: test
test: build
	cargo test --package swap-contract -- --show-output
