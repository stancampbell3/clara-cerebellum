#!/usr/bin/bash
FRONTDESK_CONFIG=clara-frontdesk-poc/config/city_of_dis.toml RUST_LOG=clara_frontdesk=debug cargo run -p clara-frontdesk-poc
