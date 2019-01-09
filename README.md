# wasm-plugins
A collection of WASM plugins for ZoKrates

## Building

Make sure that you have `nightly` and `wasm32-unknown-unknown` as a target:

```shell
$ rustup toolchain install nightly
$ rustup target add wasm32-unknown-unknown
```

Then simply build the plugins with:

```shell
$ cargo build
```

## Adding plugins

Add a directory containing the rust project for your plugin. Then edit the `Cargo.toml` at the root of this repository and add  that directory name to `exclude`.
