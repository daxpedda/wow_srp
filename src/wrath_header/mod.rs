#![allow(missing_docs)]
use std::io::{Read, Write};

pub use decrypt::ClientDecrypterHalf;
pub use decrypt::ServerDecrypterHalf;
pub use encrypt::ClientEncrypterHalf;
pub use encrypt::ServerEncrypterHalf;

use crate::error::MatchProofsError;
use crate::key::{Proof, SessionKey};
use crate::normalized_string::NormalizedString;
use crate::vanilla_header::calculate_world_server_proof;
use crate::{PROOF_LENGTH, SESSION_KEY_LENGTH};
use rand::{thread_rng, RngCore};

pub(crate) mod decrypt;
pub(crate) mod encrypt;
mod inner_crypto;

pub const CLIENT_HEADER_LENGTH: u8 =
    (std::mem::size_of::<u16>() + std::mem::size_of::<u32>()) as u8;
pub const SERVER_HEADER_LENGTH: u8 =
    (std::mem::size_of::<u16>() + std::mem::size_of::<u16>()) as u8;

// Used for Client (Encryption) to Server (Decryption)
const S: [u8; 16] = [
    0xC2, 0xB3, 0x72, 0x3C, 0xC6, 0xAE, 0xD9, 0xB5, 0x34, 0x3C, 0x53, 0xEE, 0x2F, 0x43, 0x67, 0xCE,
];

// Used for Server (Encryption) to Client (Decryption) messages
const R: [u8; 16] = [
    0xCC, 0x98, 0xAE, 0x04, 0xE8, 0x97, 0xEA, 0xCA, 0x12, 0xDD, 0xC0, 0x93, 0x42, 0x91, 0x53, 0x57,
];

#[derive(Debug, Clone, Copy)]
pub struct ServerHeader {
    pub size: u16,
    pub opcode: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ClientHeader {
    pub size: u16,
    pub opcode: u32,
}

pub struct ClientCrypto {
    decrypt: ClientDecrypterHalf,
    encrypt: ClientEncrypterHalf,
}

impl ClientCrypto {
    pub fn decrypter(&mut self) -> &mut ClientDecrypterHalf {
        &mut self.decrypt
    }

    pub fn encrypter(&mut self) -> &mut ClientEncrypterHalf {
        &mut self.encrypt
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.encrypt.encrypt(data);
    }

    pub fn write_encrypted_client_header<W: Write>(
        &mut self,
        write: &mut W,
        size: u16,
        opcode: u32,
    ) -> std::io::Result<()> {
        self.encrypt
            .write_encrypted_client_header(write, size, opcode)
    }

    pub fn encrypt_client_header(
        &mut self,
        size: u16,
        opcode: u32,
    ) -> [u8; CLIENT_HEADER_LENGTH as usize] {
        self.encrypt.encrypt_client_header(size, opcode)
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.decrypt.decrypt(data);
    }

    pub fn read_and_decrypt_server_header<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> std::io::Result<ServerHeader> {
        self.decrypt.read_and_decrypt_server_header(reader)
    }

    pub fn decrypt_server_header(
        &mut self,
        data: [u8; SERVER_HEADER_LENGTH as usize],
    ) -> ServerHeader {
        self.decrypt.decrypt_server_header(data)
    }

    pub(crate) fn new(session_key: [u8; SESSION_KEY_LENGTH as usize]) -> Self {
        Self {
            decrypt: ClientDecrypterHalf::new(session_key),
            encrypt: ClientEncrypterHalf::new(session_key),
        }
    }
}

pub struct ServerCrypto {
    decrypt: ServerDecrypterHalf,
    encrypt: ServerEncrypterHalf,
}

impl ServerCrypto {
    pub fn decrypter(&mut self) -> &mut ServerDecrypterHalf {
        &mut self.decrypt
    }

    pub fn encrypter(&mut self) -> &mut ServerEncrypterHalf {
        &mut self.encrypt
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.encrypt.encrypt(data);
    }

    pub fn write_encrypted_server_header<W: Write>(
        &mut self,
        write: &mut W,
        size: u16,
        opcode: u16,
    ) -> std::io::Result<()> {
        self.encrypt
            .write_encrypted_server_header(write, size, opcode)
    }

    pub fn encrypt_server_header(
        &mut self,
        size: u16,
        opcode: u16,
    ) -> [u8; SERVER_HEADER_LENGTH as usize] {
        self.encrypt.encrypt_server_header(size, opcode)
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.decrypt.decrypt(data);
    }

    pub fn read_and_decrypt_client_header<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> std::io::Result<ClientHeader> {
        self.decrypt.read_and_decrypt_client_header(reader)
    }

    pub fn decrypt_client_header(
        &mut self,
        mut data: [u8; CLIENT_HEADER_LENGTH as usize],
    ) -> ClientHeader {
        self.decrypt(&mut data);

        let size: u16 = u16::from_be_bytes([data[0], data[1]]);
        let opcode: u32 = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);

        ClientHeader { size, opcode }
    }

    pub(crate) fn new(session_key: [u8; SESSION_KEY_LENGTH as usize]) -> Self {
        Self {
            decrypt: ServerDecrypterHalf::new(session_key),
            encrypt: ServerEncrypterHalf::new(session_key),
        }
    }
}

pub struct ProofSeed {
    seed: u32,
}

impl ProofSeed {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn from_specific_seed(server_seed: u32) -> Self {
        Self { seed: server_seed }
    }

    pub const fn seed(&self) -> u32 {
        self.seed
    }

    pub fn into_proof_and_header_crypto(
        self,
        username: &NormalizedString,
        session_key: [u8; SESSION_KEY_LENGTH as _],
        server_seed: u32,
    ) -> ([u8; PROOF_LENGTH as _], ClientCrypto) {
        let client_proof = calculate_world_server_proof(
            username,
            &SessionKey::from_le_bytes(session_key),
            server_seed,
            self.seed,
        );

        let crypto = ClientCrypto::new(session_key);

        (*client_proof.as_le(), crypto)
    }

    pub fn into_header_crypto(
        self,
        username: &NormalizedString,
        session_key: [u8; SESSION_KEY_LENGTH as _],
        client_proof: [u8; PROOF_LENGTH as _],
        client_seed: u32,
    ) -> Result<ServerCrypto, MatchProofsError> {
        let server_proof = calculate_world_server_proof(
            username,
            &SessionKey::from_le_bytes(session_key),
            self.seed,
            client_seed,
        );

        if server_proof != Proof::from_le_bytes(client_proof) {
            return Err(MatchProofsError {
                client_proof,
                server_proof: *server_proof.as_le(),
            });
        }

        Ok(ServerCrypto::new(session_key))
    }
}

impl Default for ProofSeed {
    fn default() -> Self {
        Self {
            seed: thread_rng().next_u32(),
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs::read_to_string;

    use crate::hex::*;
    use crate::key::SessionKey;
    use crate::normalized_string::NormalizedString;
    use crate::wrath_header::ProofSeed;
    use std::convert::TryInto;

    #[test]
    fn verify_seed_proof() {
        const FILE: &'static str = "tests/encryption/calculate_world_server_proof.txt";
        let contents = read_to_string(FILE).unwrap();
        for line in contents.lines() {
            let mut line = line.split_whitespace();

            let username = line.next().unwrap();
            let session_key = SessionKey::from_be_hex_str(line.next().unwrap());
            let server_seed =
                u32::from_le_bytes(hex_decode(line.next().unwrap()).try_into().unwrap());
            let client_seed = ProofSeed::from_specific_seed(u32::from_le_bytes(
                hex_decode(line.next().unwrap()).try_into().unwrap(),
            ));
            let expected: [u8; 20] = hex_decode(line.next().unwrap()).try_into().unwrap();

            let (proof, _) = client_seed.into_proof_and_header_crypto(
                &username.try_into().unwrap(),
                *session_key.as_le(),
                server_seed,
            );

            assert_eq!(expected, proof);
        }
    }

    #[test]
    fn verify_client_and_server_agree() {
        let session_key = [
            239, 107, 150, 237, 174, 220, 162, 4, 138, 56, 166, 166, 138, 152, 188, 146, 96, 151,
            1, 201, 202, 137, 231, 87, 203, 23, 62, 17, 7, 169, 178, 1, 51, 208, 202, 223, 26, 216,
            250, 9,
        ];

        let username = NormalizedString::new("A").unwrap();

        let client_seed = ProofSeed::new();
        let client_seed_value = client_seed.seed();
        let server_seed = ProofSeed::new();

        let (client_proof, mut client_crypto) =
            client_seed.into_proof_and_header_crypto(&username, session_key, server_seed.seed());

        let mut server_crypto = server_seed
            .into_header_crypto(&username, session_key, client_proof, client_seed_value)
            .unwrap();

        let original_data = hex_decode("3d9ae196ef4f5be4df9ea8b9f4dd95fe68fe58b653cf1c2dbeaa0be167db9b27df32fd230f2eab9bd7e9b2f3fbf335d381ca");
        let mut data = original_data.clone();

        client_crypto.encrypt(&mut data);
        server_crypto.decrypt(&mut data);

        assert_eq!(original_data, data);

        server_crypto.encrypt(&mut data);
        client_crypto.decrypt(&mut data);

        assert_eq!(original_data, data);
    }

    // fn verify_server_header() { }

    // fn verify_client_header() { }

    #[test]
    fn verify_login() {
        let session_key = [
            115, 0, 100, 222, 18, 15, 156, 194, 27, 1, 216, 229, 165, 207, 78, 233, 183, 241, 248,
            73, 190, 142, 14, 89, 44, 235, 153, 190, 103, 206, 34, 88, 45, 199, 104, 175, 79, 108,
            93, 48,
        ];
        let username = NormalizedString::new("A").unwrap();
        let server_seed = 0xDEADBEEF;
        let client_seed = 1266519981;
        let client_proof = [
            202, 54, 102, 180, 90, 87, 9, 107, 217, 97, 235, 56, 221, 203, 108, 19, 109, 141, 137,
            7,
        ];

        let seed = ProofSeed::from_specific_seed(server_seed);
        let encryption = seed.into_header_crypto(&username, session_key, client_proof, client_seed);
        assert!(encryption.is_ok());
    }

    // fn verify_encrypt() { }

    // fn verify_mixed_used() { }

    // fn verify_trait_helpers() { }

    // fn verify_decrypt() { }
}
