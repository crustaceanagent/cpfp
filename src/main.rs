use bdk_kyoto::builder::{Builder, BuilderExt};
use bdk_kyoto::ScanType;
use bdk_wallet::bitcoin::amount::Amount;
use bdk_wallet::bitcoin::bip32::{ChildNumber, Xpriv};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::{Network, ScriptBuf};
use bdk_wallet::{KeychainKind, SignOptions, Wallet};
use bitcoin::OutPoint;
use std::net::SocketAddr;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Deterministic entropy for reproducible wallet generation
    let entropy = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
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
    let output = Command::new("./mine_blocks.sh").arg(&address).output()?;

    if !output.status.success() {
        eprintln!("Error running mine_blocks.sh:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    } else {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    // Build the light client using bdk_kyoto with local bitcoind as peer
    let local_peer: SocketAddr = "127.0.0.1:18444".parse()?;
    let client = Builder::new(Network::Regtest)
        .add_peer(local_peer)
        .build_with_wallet(&wallet, ScanType::Sync)?;

    let (client, _, mut update_subscriber) = client.subscribe();
    client.start();

    println!("Syncing via P2P network...");

    // Wait for sync to complete
    let update = tokio::time::timeout(
        tokio::time::Duration::from_secs(60),
        update_subscriber.update(),
    )
    .await??;

    wallet.apply_update(update)?;

    println!("Wallet synced!");
    println!("Balance: {}", wallet.balance());

    // === PARENT TRANSACTION ===
    println!("\n[*] Building parent transaction...");

    let op_return_data = b"CPFP - Child Pays For Parent";
    let op_return_script = ScriptBuf::new_op_return(op_return_data);
    let bumper = wallet.reveal_next_address(KeychainKind::External);

    let mut builder = wallet.build_tx();
    builder.add_recipient(op_return_script, Amount::from_sat(1000));
    builder.add_recipient(bumper.script_pubkey(), Amount::from_sat(50000));
    builder.fee_rate(bdk_wallet::bitcoin::FeeRate::from_sat_per_vb(1).unwrap());
    let mut psbt = builder.finish()?;

    let sign_result = wallet.sign(&mut psbt, SignOptions::default())?;
    println!("Parent transaction signed: {}", sign_result);

    let parent = psbt.extract_tx().unwrap();
    println!("\n=== Parent Transaction Metadata ===");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    wallet.apply_unconfirmed_txs(vec![(parent.clone(), now)]);

    println!("Transaction ID: {}", parent.compute_txid());
    println!("Inputs: {}", parent.input.len());
    println!("Outputs: {}", parent.output.len());
    println!();
    let mut bumping_outpoint = parent.input.first().unwrap().previous_output;

    for (i, output) in parent.output.iter().enumerate() {
        println!("Output {}:", i);
        println!(
            "  Value: {} sats ({} BTC)",
            output.value,
            output.value.to_btc()
        );
        println!("  Script: {:?}", output.script_pubkey);
        if output.script_pubkey.is_op_return() {
            println!("  Type: OP_RETURN");
        } else {
            println!("  Type: Standard");
        }
        if output.script_pubkey == bumper.script_pubkey() {
            bumping_outpoint = OutPoint {
                txid: parent.compute_txid(),
                vout: i as u32,
            };
        }
        println!();
    }

    // === CHILD TRANSACTION ===
    // Spend the change output from the parent transaction
    println!("\n[*] Building child transaction (CPFP) to spend change output...");

    // Get a new internal address for the child transaction recipient
    let child_recipient = wallet.reveal_next_address(KeychainKind::Internal);
    let child_recipient_script = child_recipient.address.script_pubkey();

    println!("Using OutPoint: {bumping_outpoint}");
    // Build child transaction spending the change UTXO
    let mut child_builder = wallet.build_tx();
    child_builder.add_utxo(bumping_outpoint)?;
    child_builder.manually_selected_only();
    child_builder.add_recipient(child_recipient_script, Amount::from_sat(1000));
    child_builder.fee_rate(bdk_wallet::bitcoin::FeeRate::from_sat_per_vb(5).unwrap()); // 5 sat/vB
    let mut child_psbt = child_builder.finish()?;

    let child_sign_result = wallet.sign(&mut child_psbt, SignOptions::default())?;
    println!("\n=== Child Transaction Metadata ===");
    println!("Signing result: {}", child_sign_result);

    let child_tx = &child_psbt.unsigned_tx;
    println!("Transaction ID: {}", child_tx.compute_txid());
    println!("Inputs: {}", child_tx.input.len());
    println!("Outputs: {}", child_tx.output.len());
    println!();

    for (i, input) in child_tx.input.iter().enumerate() {
        println!("Input {}:", i);
        println!("OutPoint: {}", input.previous_output);
        println!();
    }

    for (i, output) in child_tx.output.iter().enumerate() {
        println!("Output {}:", i);
        println!(
            "  Value: {} sats ({} BTC)",
            output.value,
            output.value.to_btc()
        );
        println!("  Script: {:?}", output.script_pubkey);
        if output.script_pubkey.is_op_return() {
            println!("  Type: OP_RETURN");
        } else {
            println!("  Type: Standard");
        }
        println!();
    }

    // === STOP BITCOIND ===
    println!("\n[*] Stopping bitcoind...");
    let stop_output = Command::new("bitcoin-cli")
        .args(["-chain=regtest", "stop"])
        .output()?;

    if stop_output.status.success() {
        println!("bitcoind stopped successfully");
    } else {
        eprintln!(
            "Error stopping bitcoind: {}",
            String::from_utf8_lossy(&stop_output.stderr)
        );
    }

    Ok(())
}
