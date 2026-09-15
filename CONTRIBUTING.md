1. Install Rust following [the official instructions](https://rust-lang.org/tools/install/).
   After installing Rust, run the following command in the project root to verify it is working as expected.
    ```bash
   rustup show
   ```
   `rustup show` reads the `./rust-toolchain.toml` file and installs the correct Rust toolchain and components for this
   project.
2. Install [binstall](https://github.com/cargo-bins/cargo-binstall) to enable fast, low-complexity usage of the
   required Cargo tools for this project.
   ```bash
    cargo install cargo-binstall
   ```
3. Install [just](https://github.com/casey/just) which is a handy way to run project-specific commands.
    ```bash
   cargo binstall just -y
    ```
4. Install Node.js and make `node` available on `PATH`. The resolver tests use Node
   to exercise the same filesystem bridge and launcher that the WASM extension uses.
5. Run `just ready` to verify that the project builds and runs correctly. Run this command before creating a PR to ensure
   that CI will pass.
