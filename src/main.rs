use bdk_wallet::bitcoin::bip32::{ChildNumber, Xpriv};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::Network;
use bdk_wallet::{KeychainKind, Wallet};
use bdk_kyoto::builder::{Builder, BuilderExt};
use bdk_kyoto::ScanType;

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
    println!("External address: {}", addr);

    // Build the light client using bdk_kyoto
    let client = Builder::new(Network::Regtest).build_with_wallet(&wallet, ScanType::Sync)?;

    let (client, _, mut update_subscriber) = client.subscribe();
    client.start();

    println!("Syncing via P2P network...");

    // Wait for sync to complete
    loop {
        tokio::select! {
            update = update_subscriber.update() => {
                let update = update?;
                wallet.apply_update(update)?;
                break;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
                println!("Timeout waiting for sync");
                break;
            }
        }
    }

    println!("Wallet synced!");
    println!("Balance: {}", wallet.balance());

    Ok(())
}