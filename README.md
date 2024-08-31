# Iroha smart contracts for Palau T-bonds

### Usage example

- build `smart_contracts/executor`: `cargo build --release`
- put `./target/wasm32-unknown-unknown/release/executor.wasm` into `configs/peer`
- docker-compose up -d
- cargo run

### Additional work

1. Currently smart contracts are registered by the client. Should they be registered in `genesis.json` or in executor migration?
