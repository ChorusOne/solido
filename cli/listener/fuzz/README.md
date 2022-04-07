# APY Daemon fuzzer

The APY daemon is internet-exposed and accepts user input, so we should fuzz it
to find any inputs that might trigger a panic (for example, overflow in the
datetime handling, etc.).

First, install `cargo-fuzz`:

    cargo +nightly-2022-03-22 install cargo-fuzz --vers 0.11.0

Then run a (slow) fuzzer in debug mode with address sanitizer, from the
`cli/listener` directory:

    cargo +nightly-2022-03-22 fuzz run apy_endpoint \
      --jobs=«num-cores» \
      -- -dict=fuzz/dictionary.txt

Or run a faster fuzzer but with less intricate coverage:

    cargo +nightly-2022-03-22 fuzz run apy_endpoint \
      --jobs=«num-cores» \
      --release \
      --sanitizer=none \
      --debug-assertions \
      -- -dict=fuzz/dictionary.txt

Note, initial compilation will seem to hang at `curve25519-dalek`, but it will
finish eventually. See also [rust-lang/rust#95240][95240]

[95240]: https://github.com/rust-lang/rust/issues/95240
