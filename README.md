# solido
Working repository for Lido for Solana 

## Usage

1. Install [Rust](https://rustup.rs/)
2. Install solana sdk: `sh -c "$(curl -sSfL https://release.solana.com/v1.6.1/install)"`
3. Add to your `.profile` of choice: `export PATH="/home/runner/.local/share/solana/install/active_release/bin:$PATH"` the correct path appears at the end of step `2`.
4. Build with `make build` and test with `make test`.