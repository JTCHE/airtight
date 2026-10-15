//! Checks against `vectors.txt`, made with Python's hashlib, hmac and the
//! `cryptography` package (SRP from a plain big-integer server side).

use airtight_crypto::{chacha, hmac, sha512::sha512, srp};

fn vectors() -> Vec<(String, Vec<String>)> {
    include_str!("vectors.txt")
        .lines()
        .map(|line| {
            let mut words = line.split_whitespace().map(String::from);
            (words.next().unwrap(), words.collect())
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn message() -> Vec<u8> {
    (0..768).map(|i| i as u8).collect()
}

fn find(name: &str) -> Vec<u8> {
    let (_, words) = vectors().into_iter().find(|(n, _)| n == name).unwrap();
    unhex(&words[0])
}

#[test]
fn sha512_matches() {
    let msg = message();
    for (name, words) in vectors().iter().filter(|(n, _)| n == "sha") {
        let length: usize = words[0].parse().unwrap();
        assert_eq!(hex(&sha512(&[&msg[..length]])), words[1], "{name} {length}");
    }
}

#[test]
fn hmac_and_hkdf_match() {
    let key = b"key".repeat(50);
    assert_eq!(hmac::hmac_sha512(&key, b"The quick brown fox").to_vec(), find("hmac"));
    let secret: Vec<u8> = (0..64).collect();
    let derived = hmac::hkdf_key(&secret, "Control-Salt", "Control-Write-Encryption-Key");
    assert_eq!(derived.to_vec(), find("hkdf"));
}

#[test]
fn aead_matches_and_round_trips() {
    let key: [u8; 32] = std::array::from_fn(|i| i as u8);
    let nonce: [u8; 12] = std::array::from_fn(|i| i as u8);
    let aad = [1, 2, 3, 4, 5, 6, 7, 8];
    let msg = message();
    for (_, words) in vectors().iter().filter(|(n, _)| n == "aead") {
        let length = words[0].parse::<usize>().unwrap().min(msg.len());
        let mut data = msg[..length].to_vec();
        let tag = chacha::seal(&key, &nonce, &aad, &mut data);
        let mut sealed = data.clone();
        sealed.extend_from_slice(&tag);
        assert_eq!(hex(&sha512(&[&sealed])), words[1], "length {length}");
        assert!(chacha::open(&key, &nonce, &aad, &mut data, &tag));
        assert_eq!(data, msg[..length]);
        data.first_mut().map(|b| *b ^= 1);
        assert!(length == 0 || !chacha::open(&key, &nonce, &aad, &mut data, &tag));
    }
}

#[test]
fn srp_agrees_with_the_server() {
    let salt: Vec<u8> = (0..16).collect();
    let private_key: Vec<u8> = (200..232).collect();
    let proof = srp::client("Pair-Setup", "3939", &salt, &find("srp_b"), &private_key).unwrap();
    assert_eq!(proof.public_key, find("srp_A"));
    assert_eq!(proof.session_key.to_vec(), find("srp_K"));
    assert_eq!(proof.client_proof.to_vec(), find("srp_M1"));
    assert_eq!(proof.server_proof.to_vec(), find("srp_HAMK"));
}
