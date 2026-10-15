//! Transient pair-setup (HomeKit SRP with the fixed PIN 3939), which
//! gives the keys for the control channel, the audio and the events channel.

use crate::channel::Channel;
use crate::hap::Cipher;
use crate::Error;
use airtight_crypto::{hmac::hkdf_key, random, srp};
use airtight_wire::tlv8;

const USERNAME: &str = "Pair-Setup";
const PIN: &str = "3939";
const TRANSIENT: u8 = 0x10;

/// The shared secret from pairing, and what derives from it.
pub struct Keys {
    secret: [u8; 64],
}

impl Keys {
    /// The control channel cipher, for our side.
    pub fn control(&self) -> Cipher {
        Cipher::new(
            hkdf_key(&self.secret, "Control-Salt", "Control-Write-Encryption-Key"),
            hkdf_key(&self.secret, "Control-Salt", "Control-Read-Encryption-Key"),
        )
    }

    /// The events channel cipher. The receiver opened its side as the
    /// writer, so the names swap.
    pub fn events(&self) -> Cipher {
        Cipher::new(
            hkdf_key(&self.secret, "Events-Salt", "Events-Read-Encryption-Key"),
            hkdf_key(&self.secret, "Events-Salt", "Events-Write-Encryption-Key"),
        )
    }

    /// The audio payload key ("shk").
    pub fn audio(&self) -> [u8; 32] {
        self.secret[..32].try_into().unwrap()
    }
}

fn post(channel: &mut Channel, body: &[u8]) -> Result<Vec<(u8, Vec<u8>)>, Error> {
    let headers = [("Content-Type", "application/octet-stream"), ("X-Apple-HKP", "4")];
    let reply = channel.expect_ok("POST", "/pair-setup", &headers, body)?;
    let items = tlv8::decode(&reply).ok_or(Error("malformed pairing reply".into()))?;
    if let Some(code) = tlv8::find(&items, tlv8::ERROR) {
        return Err(Error(format!("the receiver refused pairing (error {code:?})")));
    }
    Ok(items)
}

pub fn transient(channel: &mut Channel) -> Result<Keys, Error> {
    let m2 = post(
        channel,
        &tlv8::encode(&[(tlv8::STATE, &[1]), (tlv8::METHOD, &[0]), (tlv8::FLAGS, &[TRANSIENT])]),
    )?;
    let salt = tlv8::find(&m2, tlv8::SALT).ok_or(Error("pairing reply has no salt".into()))?;
    let server_key = tlv8::find(&m2, tlv8::PUBLIC_KEY).ok_or(Error("pairing reply has no key".into()))?;
    let private_key: [u8; 32] = random::bytes();
    let proof = srp::client(USERNAME, PIN, salt, server_key, &private_key)
        .ok_or(Error("the receiver sent an invalid key".into()))?;

    let m4 = post(
        channel,
        &tlv8::encode(&[
            (tlv8::STATE, &[3]),
            (tlv8::PUBLIC_KEY, &proof.public_key),
            (tlv8::PROOF, &proof.client_proof),
        ]),
    )?;
    if tlv8::find(&m4, tlv8::PROOF) != Some(&proof.server_proof[..]) {
        return Err(Error("the receiver's pairing proof is wrong".into()));
    }
    Ok(Keys { secret: proof.session_key })
}
