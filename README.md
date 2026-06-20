# pico_rust_template

Minimal Rust firmware template for Raspberry Pi Pico 2 / RP235x boards.

The project uses Embassy async runtime and builds for the
`thumbv8m.main-none-eabihf` bare-metal target. It is set up for flashing and
running through `probe-rs`.

## Requirements

- Rust with the `thumbv8m.main-none-eabihf` target installed
- `probe-rs`
- A Pico 2 / RP235x board connected through a supported debug probe

Install the target with:

```sh
rustup target add thumbv8m.main-none-eabihf
```

## Build

Build production firmware:

```sh
make build-prod
```

This produces the release ELF at:

```text
target/thumbv8m.main-none-eabihf/release/pico_rust_template
```

## Flash

Build and flash production firmware:

```sh
make flash-prod
```

This uses `probe-rs download --chip RP235x` and then resets the board.

## Run

Build and run the development firmware:

```sh
make run-dev
```

Development mode enables `defmt`/RTT logging and runs via the Cargo runner
configured in `.cargo/config.toml`.

## Other Commands

```sh
make size
make clean
```

## Forking and Renaming

To use this repository as the base for another Pico 2 project:

1. Fork or clone the repository.

2. Rename the package and binary in `Cargo.toml`:

```toml
[package]
name = "your_project_name"

[[bin]]
name = "your_project_name"
path = "src/main.rs"
```

3. Update the binary name and output path references in `Makefile`:

```make
cargo build --bin your_project_name ...
target/thumbv8m.main-none-eabihf/release/your_project_name
cargo run --bin your_project_name ...
```

4. Update any user-facing strings in `src/main.rs`, such as log messages and
   file comments.

5. Regenerate the lockfile after renaming:

```sh
cargo update
```

After that, the same commands should work with the new project name:

```sh
make build-prod
make flash-prod
make run-dev
```
