# SPDX-FileCopyrightText: 2025 TriliTech <contact@trili.tech>
#
# SPDX-License-Identifier: MIT

### Generic top-level targets

all: riscv/all jstz/all dummy/all block-cache-tester/all etherlink/all revm/all

build-deps: riscv/build-deps jstz/build-deps etherlink/build-deps revm/build-deps

build-deps-slim: riscv/build-deps-slim

check: riscv/check jstz/check dummy/check block-cache-tester/all etherlink/check revm/check

audit: riscv/audit

build: riscv/build jstz/build dummy/build block-cache-tester/build etherlink/build revm/build revm/inbox-bench

run-revm: revm/build
	@cargo run --manifest-path=kernels/revm/Cargo.toml --bin inbox-bench --release -- generate --address "sr163Lv22CdE8QagCwf48PWDTquk6isQwv57" --transfers 16
	@cargo run --manifest-path src/riscv/Cargo.toml --release -- run -m supervisor --address "sr163Lv22CdE8QagCwf48PWDTquk6isQwv57" --input kernels/revm/target/riscv64gc-unknown-linux-musl/release/revm-kernel --inbox-file inbox.json

test: riscv/test jstz/test etherlink/test revm/test

test-long: riscv/test-long

test-miri: riscv/test-miri

clean: riscv/clean jstz/clean dummy/clean block-cache-tester/clean etherlink/clean revm/clean

### Target proxies

riscv/%:
	@make -C src/riscv ${@:riscv/%=%}

jstz/%:
	@make -C kernels/jstz ${@:jstz/%=%}

dummy/%:
	@make -C kernels/dummy ${@:dummy/%=%}

block-cache-tester/%:
	@make -C kernels/block-cache-tester ${@:block-cache-tester/%=%}

etherlink/%:
	@make -C kernels/etherlink ${@:etherlink/%=%}

revm/%:
	@make -C kernels/revm ${@:revm/%=%}

# Mark all non-pattern targets as phony to make sure they're always executed
.PHONY: all build-deps build-deps-slim check audit build test test-long test-miri clean
