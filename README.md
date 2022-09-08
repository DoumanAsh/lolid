# lolid

[![Rust](https://github.com/DoumanAsh/lolid/actions/workflows/rust.yml/badge.svg)](https://github.com/DoumanAsh/lolid/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/lolid.svg)](https://crates.io/crates/lolid)
[![Documentation](https://docs.rs/lolid/badge.svg)](https://docs.rs/crate/lolid/)

Minimal `no_std` UUID implementation.

## Features:

- `md5`   - Enables v3;
- `orng`  - Enables v4 using OS random, allowing unique UUIDs;
- `prng`  - Enables v4 using pseudo random, allowing unique, but predictable UUIDs;
- `sha1`  - Enables v5;
- `serde` - Enables `serde` support;
- `std`   - Enables usages of `std` facilities like getting current time.
