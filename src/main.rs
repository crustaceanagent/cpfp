use bdk_wallet::bitcoin::bip32::{ChildNumber, Xpriv};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::{Network, ScriptBuf};
use bdk_wallet::bitcoin::amount::Amount;
use bdk_wallet::{KeychainKind, Wallet, SignOptions};
use bdk_kyoto::builder::{Builder, BuilderExt};
use bdk_kyoto::ScanType;
use std::net::SocketAddr;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Deterministic entropy for reproducible wallet generation
    let entropy = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    ];
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)?;
    let seed = mnemonic.to_seed("");

    // Derive the extended private key for m/84'/0'/0'
    let ext_priv = Xpriv::new_master(Network::Regtest, &seed)?;

    // Derive to account level (m/84'/0'/0')
    let secp = Secp256k1::new();
    let account_priv = ext_priv.derive_priv(
        &secp,
        &[
            ChildNumber::from_hardened_idx(84)?,
            ChildNumber::from_hardened_idx(0)?,
            ChildNumber::from_hardened_idx(0)?,
        ],
    )?;

    // Get the encoded xprv as a string (Display impl does base58check)
    let account_xprv = account_priv.to_string();

    // Create descriptors for external and internal keychains
    let external_descriptor = format!("wpkh({}/0/*)", account_xprv);
    let internal_descriptor = format!("wpkh({}/1/*)", account_xprv);

    println!("External descriptor: {}", external_descriptor);
    println!("Internal descriptor: {}", internal_descriptor);

    // Create the wallet without persistence
    let mut wallet = Wallet::create(external_descriptor, internal_descriptor)
        .network(Network::Regtest)
        .create_wallet_no_persist()?;

    println!("Wallet created successfully!");
    let addr = wallet.reveal_next_address(KeychainKind::External);
    let address = addr.address.to_string();
    println!("External address: {}", address);

    // Mine blocks to the address
    println!("\n[*] Mining blocks to address...");
    let output = Command::new("./mine_blocks.sh")
        .arg(&address)
        .output()?;

    if !output.status.success() {
        eprintln!("Error running mine_blocks.sh:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    } else {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    // Build the light client using bdk_kyoto with local bitcoind as peer
    let local_peer: SocketAddr = "127.0.0.1:18433".parse()?;
    let client = Builder::new(Network::Regtest)
        .add_peer(local_peer)
        .build_with_wallet(&wallet, ScanType::Sync)?;

    let (client, _, mut update_subscriber) = client.subscribe();
    client.start();

    println!("Syncing via P2P network...");

    // Wait for sync to complete
    let update = tokio::time::timeout(
        tokio::time::Duration::from_secs(60),
        update_subscriber.update()
    ).await??;

    wallet.apply_update(update)?;

    println!("Wallet synced!");
    println!("Balance: {}", wallet.balance());

    // Build transaction with OP_RETURN
    println!("\n[*] Building transaction with OP_RETURN...");

    // Create OP_RETURN script with some data
    let op_return_data = b"CPFP - Child Pays For Parent";
    let op_return_script = ScriptBuf::new_op_return(op_return_data);

    // Build transaction step by step
    let mut builder = wallet.build_tx();
    builder.add_recipient(op_return_script, Amount::from_sat(1000));
    builder.fee_rate(bdk_wallet::bitcoin::FeeRate::from_sat_per_vb(1).unwrap());
    let mut psbt = builder.finish()?;

    // Get transaction details
    let tx = &psbt.unsigned_tx;
    let input_count = tx.input.len();
    let output_count = tx.output.len();

    // Print transaction metadata
    println!("\n=== Transaction Metadata ===");
    println!("Transaction ID: {}", tx.compute_txid());
    println!("Inputs: {}", input_count);
    println!("Outputs: {}", output_count);
    println!();

    for (i, output) in tx.output.iter().enumerate() {
        println!("Output {}:", i);
        println!("  Value: {} sats ({} BTC)", output.value, output.value.to_btc());
        println!("  Script: {:?}", output.script_pubkey);
        if output.script_pubkey.is_op_return() {
            println!("  Type: OP_RETURN");
        } else {
            println!("  Type: Standard (change)");
        }
        println!();
    }

    // Sign the transaction
    let sign_result = wallet.sign(&mut psbt, SignOptions::default())?;

    println!("\n=== Signed Transaction ===");
    println!("Signing result: {}", sign_result);
    println!("Transaction fully signed: {}", sign_result);

    // Extract the final transaction
    let final_tx = psbt.extract_tx()?;
    use bdk_wallet::bitcoin::consensus::Encodable;
    let mut bytes = Vec::new();
    final_tx.consensus_encode(&mut bytes)?;
    println!("\nRaw transaction (hex):");
    println!("{}", hex::encode(bytes));

    println!("\n=== Summary ===");
    println!("Sent 1000 satoshis to OP_RETURN");
    println!("Transaction NOT broadcast (as requested)");

    Ok(())
}