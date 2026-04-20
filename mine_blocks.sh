#!/usr/bin/env bash

set -euo pipefail

# --- Validate arguments ---
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <bitcoin-address>"
    exit 1
fi

BTC_ADDRESS="$1"

# --- Basic address format sanity check ---
if [[ ! "$BTC_ADDRESS" =~ ^(bcrt1|m|n|2)[a-zA-Z0-9]{25,62}$ ]]; then
    echo "Error: '$BTC_ADDRESS' does not look like a valid regtest address."
    echo "Regtest addresses typically start with 'bcrt1', 'm', 'n', or '2'."
    exit 1
fi

# --- Configuration ---
BITCOIN_CLI="bitcoin-cli"
BITCOIND="bitcoind"
REGTEST_DATA_DIR="${HOME}/.bitcoin/regtest"
NUM_BLOCKS=200
RPC_PORT=18444

# --- Start bitcoind in regtest mode with all debug categories ---
echo "[*] Starting bitcoind in regtest mode with full debug logging..."
"$BITCOIND" \
    -regtest \
    -daemon \
    -debug=all \
    -rpcport="${RPC_PORT}" \
    -fallbackfee=1.0 \
    -maxtxfee=1.1 \
    -server=1 \
    -blockfilterindex=1 \
    -peerblockfilters=1 \
    -peerbloomfilters=1 \
    -listen=1 \
    -bind=0.0.0.0:18433

# --- Wait for bitcoind to become ready ---
echo "[*] Waiting for bitcoind to be ready..."
MAX_WAIT=30
WAITED=0
until "$BITCOIN_CLI" -regtest -rpcport="${RPC_PORT}" ping 2>/dev/null; do
    if [ "$WAITED" -ge "$MAX_WAIT" ]; then
        echo "Error: bitcoind did not start within ${MAX_WAIT} seconds."
        exit 1
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done
echo "[*] bitcoind is ready."

# --- Create a default wallet if none exists ---
WALLET_LIST=$("$BITCOIN_CLI" -regtest -rpcport="${RPC_PORT}" listwallets 2>/dev/null)
if echo "$WALLET_LIST" | grep -q '$$$$'; then
    echo "[*] No wallet loaded. Creating a default wallet..."
    "$BITCOIN_CLI" -regtest -rpcport="${RPC_PORT}" \
        createwallet "default" false false "" false false true
fi

# --- Mine blocks to the specified address ---
echo "[*] Mining ${NUM_BLOCKS} blocks to address: ${BTC_ADDRESS}"
BLOCK_HASHES=$("$BITCOIN_CLI" \
    -regtest \
    -rpcport="${RPC_PORT}" \
    generatetoaddress "${NUM_BLOCKS}" "${BTC_ADDRESS}")

echo "[*] Successfully mined ${NUM_BLOCKS} blocks."
echo ""
echo "--- First and last block hashes ---"
FIRST_HASH=$(echo "$BLOCK_HASHES" | grep -m1 '"' | tr -d ' ",')
LAST_HASH=$(echo  "$BLOCK_HASHES" | tail -n2 | grep '"'  | tr -d ' ",')
echo "  First : ${FIRST_HASH}"
echo "  Last  : ${LAST_HASH}"

echo ""
echo "[*] Done. Run the following to check block count:"
echo "    bitcoin-cli -regtest getblockcount"
echo "[*] Debug log is at: ${REGTEST_DATA_DIR}/debug.log"