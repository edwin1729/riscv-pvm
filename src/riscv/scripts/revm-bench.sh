#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2024-2025 TriliTech <contact@trili.tech>
#
# SPDX-License-Identifier: MIT

# Build and run the revm TPS benchmark with the specified number of transfers

set -e

USAGE="Usage: -t <num_transfers> [ -s: static inbox ] [ -p: profile with samply ] [ -n: run natively ] [ -i <num_iterations>: number of runs ] [ -j: enable inline jit ] [ -m <all | jit-unsupported>: enable metrics ]"
DEFAULT_ROLLUP_ADDRESS="sr163Lv22CdE8QagCwf48PWDTquk6isQwv57"

ITERATIONS="1"
TX=""
STATIC_INBOX=""
SANDBOX_BIN="riscv-sandbox"
SANDBOX_ENABLE_FEATURES=()
PROFILING_WRAPPER=""
SAMPLY_OUT="riscv-sandbox-profile.json"
METRICS=""
METRICS_ARGS=()
NATIVE=""
REVM_SANDBOX_PARAMS=("--input" "revm/target/riscv64gc-unknown-linux-musl/release/revm-kernel")

CURR=$PWD
RISCV_DIR=$(dirname "$0")/..
cd "$RISCV_DIR"

while getopts "i:t:m:spnj" OPTION; do
  case "$OPTION" in
  i)
    ITERATIONS="$OPTARG"
    ;;
  t)
    TX="$OPTARG"
    ;;
  s)
    STATIC_INBOX="y"
    ;;
  p)
    SANDBOX_BIN="riscv-sandbox.prof"
    PROFILING_WRAPPER="samply record -s -o $SAMPLY_OUT"
    ;;
  n)
    NATIVE=$(make --silent -C revm print-native-target | grep -wv make)
    ;;
  j)
    SANDBOX_ENABLE_FEATURES+=("inline-jit")
    ;;
  m)
    SANDBOX_ENABLE_FEATURES+=("metrics")
    METRICS="y"

    case "$OPTARG" in
    all) ;;
    jit-unsupported)
      METRICS_ARGS+=("--exclude-supported-instructions")
      ;;
    *)
      echo "$USAGE"
      exit 1
      ;;
    esac
    ;;
  *)
    echo "$USAGE"
    exit 1
    ;;
  esac
done

if [ "$TX" = "" ]; then
  echo "$USAGE"
  exit 1
fi

if [ "$NATIVE" != "" ] && [ "$STATIC_INBOX" = "" ]; then
  echo "Native compilation without static inbox unsupported"
  echo "$USAGE"
  exit 1
fi

echo "[INFO]: building sandbox"
make "SANDBOX_ENABLE_FEATURES=${SANDBOX_ENABLE_FEATURES[*]}" "$SANDBOX_BIN" &>/dev/null
echo "[INFO]: building bench tool"
make -C revm inbox-bench &>/dev/null

DATA_DIR=${DATA_DIR:=$(mktemp -d)}
echo "$DATA_DIR"

echo "[INFO]: generating $TX transfers"
INBOX_FILE="${DATA_DIR}/inbox.json"
RUN_INBOX="$INBOX_FILE"
./revm/inbox-bench generate --inbox-file "$INBOX_FILE" --transfers "$TX"

log_file_args=()

BLOCK_METRICS_FILE="${DATA_DIR}/block-metrics.out"
if [ "$METRICS" != "" ]; then
  METRICS_ARGS+=("--block-metrics-file" "$BLOCK_METRICS_FILE")
fi

##########
# RISC-V #
##########
build_revm_riscv() {
  if [ "$STATIC_INBOX" = "y" ]; then
    INBOX_FILE="$INBOX_FILE" make -C revm build-kernel-static &>/dev/null
    RUN_INBOX="$DATA_DIR"/empty.json
    echo "[]" >"$RUN_INBOX"
  else
    make -C revm build-kernel &>/dev/null
  fi
}

run_revm_riscv() {
  LOG="$DATA_DIR/log.$1.log"
  $PROFILING_WRAPPER "./$SANDBOX_BIN" run \
    "${REVM_SANDBOX_PARAMS[@]}" \
    --inbox-file "$RUN_INBOX" \
    --address "$DEFAULT_ROLLUP_ADDRESS" \
    "${METRICS_ARGS[@]}" \
    --timings >"$LOG"
  log_file_args+=("--log-file=$LOG")
}

##########
# Native #
##########
build_revm_native() {
  INBOX_FILE=$INBOX_FILE make -C revm build-kernel-native &>/dev/null
}

run_revm_native() {
  LOG="$DATA_DIR/log.$1.log"
  $PROFILING_WRAPPER ./revm/target/"$NATIVE"/release/revm-kernel \
    --timings >"$LOG" 2>/dev/null
  log_file_args+=("--log-file=$LOG")
}

#########
# Build #
#########
echo "[INFO]: building revm"

if [ "$NATIVE" = "" ]; then
  build_revm_riscv
  echo "[INFO]: running $TX transfers (riscv) "
else
  build_revm_native
  echo "[INFO]: running $TX transfers ($NATIVE) "
fi

#################
# Run & Collect #
#################
run_revm() {
  echo -ne "\r\033[2K[INFO]: Run $1 / $ITERATIONS"
  if [ "$NATIVE" = "" ]; then
    run_revm_riscv "$1"
  else
    run_revm_native "$1"
  fi

  if [ "$PROFILING_WRAPPER" != "" ]; then
    echo -e "\n[INFO]: Samply data saved to: $SAMPLY_OUT"
  fi
}

collect() {
  echo -e "\033[1m"
  ./revm/inbox-bench results --inbox-file "$INBOX_FILE" "${log_file_args[@]}" --expected-transfers "$TX"
  echo -e "\033[0m"
}

for i in "$(seq "$ITERATIONS")"; do
  run_revm "$i"
done

collect

# This loads the profile of the last run
if [ -n "$PROFILING_WRAPPER" ]; then
  echo "[INFO]: collecting results"
  samply load "$SAMPLY_OUT"
fi

if [ "$METRICS" != "" ]; then
  echo "Block metrics at ${BLOCK_METRICS_FILE}"
fi

cd "$CURR"
