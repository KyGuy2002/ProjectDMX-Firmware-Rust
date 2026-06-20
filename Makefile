.PHONY: build-prod flash-prod run-dev size clean

# Production firmware
build-prod:
	cargo build --bin pico_rust_template --release --no-default-features --features production
flash-prod: build-prod
	probe-rs download --chip RP235x target/thumbv8m.main-none-eabihf/release/pico_rust_template
	probe-rs reset --chip RP235x

# For integration tests (which relies on RTT logging for additional checks)
build-dev:
	cargo build --bin pico_rust_template --no-default-features --features dev
run-dev: build-dev
	cargo run --bin pico_rust_template --no-default-features --features dev

# For memory report
size: build-prod
	./scripts/firmware_mem_size.sh target/thumbv8m.main-none-eabihf/release/pico_rust_template

clean:
	cargo clean
