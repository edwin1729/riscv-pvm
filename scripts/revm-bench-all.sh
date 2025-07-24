#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2024-2025 TriliTech <contact@trili.tech>
#
# SPDX-License-Identifier: MIT

# Build and run the revm TPS benchmark with the specified number of transfers
cd "$(dirname "$0")"

echo "------------------------------"
echo "RISC-V experiments"
echo "------------------------------"

echo "baseline"
./revm-bench.sh -t 1000 | grep -v '^\[INFO\]:'
echo "parallel signature verification (batch size 16)"
./revm-bench.sh -t 1000 -c | grep -v '^\[INFO\]:'
echo "no signature verification"
./revm-bench.sh -t 1000 -u | grep -v '^\[INFO\]:'
echo "in-memory hashmap"
./revm-bench.sh -t 1000 -h | grep -v '^\[INFO\]:'
echo "both of above"
./revm-bench.sh -t 1000 -uh | grep -v '^\[INFO\]:'


echo "------------------------------"
echo "Native experiments"
echo "------------------------------"

echo "baseline"
./revm-bench.sh -t 10000 -sn | grep -v '^\[INFO\]:'
echo "parallel signature verification (batch size 16)"
./revm-bench.sh -t 1000 -csn | grep -v '^\[INFO\]:'
echo "no signature verification"
./revm-bench.sh -t 10000 -usn | grep -v '^\[INFO\]:'
echo "in-memory hashmap"
./revm-bench.sh -t 10000 -hsn | grep -v '^\[INFO\]:'
echo "both of above"
./revm-bench.sh -t 10000 -uhsn | grep -v '^\[INFO\]:'

