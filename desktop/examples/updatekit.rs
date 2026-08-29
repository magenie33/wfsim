//! Key generation and manifest signing for the update channel.
//!
//! AN EXAMPLE, NOT A BIN, and that is not a stylistic choice. Tauri's bundler
//! picks one binary out of the crate to package, and with two to choose from it
//! picked this one: it took `updatekit`, renamed it to `wfsim-desktop` and
//! shipped a 288 KB installer that installs cleanly and contains the signing
//! tool instead of the app. `mainBinaryName` renames the choice,
//! it does not make it — setting it produced the same wrong binary under the
//! right name. An example is invisible to `cargo build --bins`, so the crate
//! has exactly one binary and the bundler cannot get it wrong.
//!
//! Kept out of the shipped binary's path on purpose: this is the half that
//! holds the PRIVATE key, and it runs on a development machine or in CI, never
//! on a reader's. `updatekit keygen` writes the private key where git cannot
//! see it and prints the public key to paste into `update.rs`; `updatekit sign`
//! is what the release job runs.
//!
//! THE PRIVATE KEY IS THE ONE UNRECOVERABLE THING IN THIS PROJECT. Losing it
//! means never being able to update an installed client again — every reader
//! is frozen on the version they have, and the only way out is asking them to
//! download an installer by hand, which is exactly the outcome the whole design
//! exists to avoid. Back it up somewhere that is not this machine.
use std::io::Write;
use std::path::PathBuf;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

fn key_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("private")
        .join("wfsim_update_key")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err("odd-length hex".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") => keygen(),
        Some("sign") => sign(args.get(2).expect("usage: updatekit sign <file>")),
        _ => {
            eprintln!("usage:\n  updatekit keygen\n  updatekit sign <file>");
            std::process::exit(2);
        }
    }
}

fn keygen() {
    let path = key_path();
    if path.exists() {
        eprintln!(
            "{} already exists — refusing to overwrite.\n\
             A NEW KEY ORPHANS EVERY INSTALLED CLIENT: they verify against the old\n\
             public key and will reject everything signed with this one. Delete it by\n\
             hand only if you know no client has it.",
            path.display()
        );
        std::process::exit(1);
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("no system randomness");
    let signing = SigningKey::from_bytes(&seed);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir private/");
    let mut f = std::fs::File::create(&path).expect("write key");
    f.write_all(hex(&seed).as_bytes()).expect("write key");
    println!("private key -> {}", path.display());
    println!("\npaste this into desktop/src/update.rs as PUBLIC_KEY:\n");
    println!("{}", hex(signing.verifying_key().as_bytes()));
    println!("\nback the private key up somewhere that is not this machine.");
}

fn sign(file: &str) {
    let seed = unhex(&std::fs::read_to_string(key_path()).expect("read private key")).expect("bad key");
    let signing = SigningKey::from_bytes(&seed.try_into().expect("key is not 32 bytes"));
    let body = std::fs::read(file).expect("read file to sign");
    // OVER THE RAW BYTES, never over a re-serialized structure: two JSON
    // encoders disagree about key order and whitespace, and a signature that
    // depends on which one ran is a signature that fails at random.
    let sig = signing.sign(&body);
    let out = format!("{file}.sig");
    std::fs::write(&out, hex(&sig.to_bytes())).expect("write signature");
    let vk: VerifyingKey = signing.verifying_key();
    println!("signed {} ({} bytes) -> {}", file, body.len(), out);
    println!("public key {}", hex(vk.as_bytes()));
}
