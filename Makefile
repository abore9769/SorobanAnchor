WASM_TARGET := wasm32-unknown-unknown
WASM_OUT    := target/$(WASM_TARGET)/release/anchorkit.wasm

.PHONY: build test wasm lint

build:
	cargo build --release

test:
	cargo test

wasm:
	cargo build --release --target $(WASM_TARGET) --no-default-features --features wasm
	@ls -lh $(WASM_OUT)

lint:
	cargo clippy -- -D warnings
