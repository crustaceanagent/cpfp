use bdk_wallet::bitcoin::bip32::{ChildNumber, Xpriv};
use bdk_wallet::bitcoin::secp256k1::Secp256k1;
use bdk_wallet::bitcoin::Network;
use bdk_wallet::KeychainKind;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Deterministic entropy for reproducible wallet generation
    let entropy = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    ];
    let mnemonic = bip39::Mnemonic::from_entropy(&entropy)?;
    let seed = mnemonic.to_seed("");

    // Derive the extended private key for m/84'/0'/0'
    let ext_priv = Xpriv::new_master(Network::Bitcoin, &seed)?;

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
    let mut wallet = bdk_wallet::Wallet::create(external_descriptor, internal_descriptor)
        .network(Network::Bitcoin)
        .create_wallet_no_persist()?;

    println!("Wallet created successfully!");
    println!("External address: {}", wallet.reveal_next_address(KeychainKind::External));

    Ok(())
}