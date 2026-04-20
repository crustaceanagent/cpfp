#!/usr/bin/env bash

set -euo pipefail

# --- Validate arguments ---
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <bitcoin-address>"
    exit 1
fi

BTC_ADDRESS="$1"

# --- Configuration ---
BITCOIN_CLI="bitcoin-cli"
BITCOIND="bitcoind"
REGTEST_DATA_DIR="${HOME}/.bitcoin/regtest"
NUM_BLOCKS=1000
RPC_PORT=18443

# --- Start bitcoind in regtest mode with all debug categories ---
echo "[*] Starting bitcoind in regtest mode with full debug logging..."
"$BITCOIND" \
    -chain=regtest \
    -daemon \
    -debug=net \
    -rpcport="${RPC_PORT}" \
    -server=1 \
    -blockfilterindex=1 \
    -peerblockfilters=1 \
    -listen=1 \
    -bind=0.0.0.0:18444

sleep 5

# --- Mine blocks to the specified address ---
echo "[*] Mining ${NUM_BLOCKS} blocks to address: ${BTC_ADDRESS}"
BLOCK_HASHES=$("$BITCOIN_CLI" \
    -chain=regtest \
    -rpcport="${RPC_PORT}" \
    generatetoaddress "${NUM_BLOCKS}" "${BTC_ADDRESS}")
