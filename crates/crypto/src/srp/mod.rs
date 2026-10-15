//! Client side of SRP-6a with SHA-512 and the RFC 5054 3072-bit group, as
//! HomeKit pair-setup uses it.

use crate::bignum::{Big, Montgomery};
use crate::sha512::{OUTPUT, sha512};

const MODULUS_HEX: [&str; 12] = [
    "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74",
    "020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F1437",
    "4FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7ED",
    "EE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF05",
    "98DA48361C55D39A69163FA8FD24CF5F83655D23DCA3AD961C62F356208552BB",
    "9ED529077096966D670C354E4ABC9804F1746C08CA18217C32905E462E36CE3B",
    "E39E772C180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF695581718",
    "3995497CEA956AE515D2261898FA051015728E5A8AAAC42DAD33170D04507A33",
    "A85521ABDF1CBA64ECFB850458DBEF0A8AEA71575D060C7DB3970F85A6E1E4C7",
    "ABF5AE8CDB0933D71E8C94E04A25619DCEE3D2261AD2EE6BF12FFA06D98A0864",
    "D87602733EC86A64521F2B18177B200CBBE117577A615D6C770988C0BAD946E2",
    "08E24FA074E5AB3143DB5BFCE0FD108E4B82D120A93AD2CAFFFFFFFFFFFFFFFF",
];
const GENERATOR: u64 = 5;
pub const MODULUS_BYTES: usize = 384;

pub struct Proof {
    /// Our public key A, minimal big-endian bytes.
    pub public_key: Vec<u8>,
    /// The client proof M1.
    pub client_proof: [u8; OUTPUT],
    /// The proof the server must answer with (HAMK).
    pub server_proof: [u8; OUTPUT],
    /// The shared session key K = H(S).
    pub session_key: [u8; OUTPUT],
}

fn modulus() -> Big {
    let hex: String = MODULUS_HEX.concat();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    Big::from_be(&bytes)
}

fn hash_padded(a: &Big, b: &Big) -> Big {
    Big::from_be(&sha512(&[&a.to_be_padded(MODULUS_BYTES), &b.to_be_padded(MODULUS_BYTES)]))
}

/// Run the client computation. `private_key` is the secret a (256 random
/// bits). None when the server's key is invalid.
pub fn client(
    username: &str,
    password: &str,
    salt: &[u8],
    server_key: &[u8],
    private_key: &[u8],
) -> Option<Proof> {
    let n = modulus();
    let field = Montgomery::new(&n);
    let g = Big::from_u64(GENERATOR);
    let b = Big::from_be(server_key);
    if field.reduce(&b).is_zero() {
        return None;
    }
    let a = Big::from_be(private_key);
    let a_public = field.power(&g, &a);
    let k = hash_padded(&n, &g);
    let u = hash_padded(&a_public, &b);
    if u.is_zero() {
        return None;
    }
    let inner = sha512(&[format!("{username}:{password}").as_bytes()]);
    let x = Big::from_be(&sha512(&[salt, &inner]));

    let kv = field.multiply_mod(&k, &field.power(&g, &x));
    let b_reduced = field.reduce(&b);
    let base = if b_reduced >= kv { b_reduced.sub(&kv) } else { b_reduced.add(&n).sub(&kv) };
    let shared = field.power(&base, &a.add(&u.mul(&x)));
    let session_key = sha512(&[&shared.to_be()]);

    let hash_n = sha512(&[&n.to_be()]);
    let hash_g = sha512(&[&g.to_be()]);
    let xored: Vec<u8> = hash_n.iter().zip(hash_g).map(|(p, q)| p ^ q).collect();
    let hash_user = sha512(&[username.as_bytes()]);
    let public_key = a_public.to_be();
    let client_proof = sha512(&[
        &xored, &hash_user, salt, &public_key, &b.to_be(), &session_key,
    ]);
    let server_proof = sha512(&[&public_key, &client_proof, &session_key]);
    Some(Proof { public_key, client_proof, server_proof, session_key })
}
