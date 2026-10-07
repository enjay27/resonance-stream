//! Checks signed metadata the way the app will before it uses it: signed by one of the keys built into
//! the app, for the revision the file names, not older than the last one published, and the dictionary
//! the one the metadata names by its SHA-256.
//!
//! `metadata.yml` runs it on the freshly signed files, so a publication the app would refuse (the wrong
//! private key in the secret, a wrong revision, a dictionary that does not match) never reaches the
//! `metadata` branch.
//!
//!     cargo run --release -p resonance-core --example verify_metadata -- \
//!         <metadata.json> <metadata.json.sig> <custom_dict.json> [<last published revision>]

use resonance_core::signed_metadata::verify_published;
use resonance_core::update_signature::TRUSTED_UPDATE_KEYS;
use std::process::ExitCode;
use std::{env, fs};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (metadata, sig, dictionary, last) = match args.as_slice() {
        [metadata, sig, dictionary] => (metadata, sig, dictionary, "0"),
        [metadata, sig, dictionary, last] => (metadata, sig, dictionary, last.as_str()),
        _ => {
            eprintln!(
                "usage: verify_metadata <metadata.json> <metadata.json.sig> <custom_dict.json> [<last revision>]"
            );
            return ExitCode::from(2);
        }
    };
    let Ok(last) = last.parse::<u64>() else {
        return fail(format!("the last revision is not a number: {last}"));
    };
    let body = match fs::read(metadata) {
        Ok(body) => body,
        Err(e) => return fail(format!("cannot read {metadata}: {e}")),
    };
    let signature = match fs::read_to_string(sig) {
        Ok(text) => text,
        Err(e) => return fail(format!("cannot read {sig}: {e}")),
    };
    let dictionary_bytes = match fs::read(dictionary) {
        Ok(bytes) => bytes,
        Err(e) => return fail(format!("cannot read {dictionary}: {e}")),
    };
    let verified = match verify_published(
        &body,
        &signature,
        &dictionary_bytes,
        TRUSTED_UPDATE_KEYS,
        last,
    ) {
        Ok(verified) => verified,
        Err(e) => return fail(e.to_string()),
    };
    println!(
        "ok: revision {} is signed by an app key and its dictionary matches (model {})",
        verified.revision, verified.model.latest_version
    );
    ExitCode::SUCCESS
}

fn fail(message: String) -> ExitCode {
    eprintln!("verify_metadata: {message}");
    ExitCode::FAILURE
}
