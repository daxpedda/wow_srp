use wow_srp::client::SrpClientUser;
use wow_srp::normalized_string::NormalizedString;
use wow_srp::server::SrpVerifier;
use wow_srp::{PublicKey, GENERATOR, LARGE_SAFE_PRIME_LITTLE_ENDIAN};

#[test]
fn authenticate_with_self() {
    let username: NormalizedString = NormalizedString::new("A").unwrap();
    let password: NormalizedString = NormalizedString::new("A").unwrap();
    let client = SrpClientUser::new(username, password);

    let username: NormalizedString = NormalizedString::new("A").unwrap();
    let password: NormalizedString = NormalizedString::new("A").unwrap();
    let verifier = SrpVerifier::from_username_and_password(username, password);

    let password_verifier = hex::encode(&verifier.password_verifier());
    let client_salt = hex::encode(&verifier.salt());

    let server = verifier.into_proof();

    let server_salt = hex::encode(&server.salt());
    let server_public_key = hex::encode(&server.server_public_key());

    let client = client.into_challenge(
        GENERATOR,
        LARGE_SAFE_PRIME_LITTLE_ENDIAN,
        PublicKey::from_le_bytes(server.server_public_key()).unwrap(),
        *server.salt(),
    );
    let client_public_key = *client.client_public_key();

    let mut server = match server.into_server(
        PublicKey::from_le_bytes(&client_public_key).unwrap(),
        &client.client_proof(),
    ) {
        Ok(s) => s,
        Err(e) => {
            panic!(
                "'{}'\
                \nverifier: {}\
                \nclient_salt: {}\
                \nserver_salt: {}\
                \nserver_public_key: {}\
                \nclient_public_key: {}",
                e,
                password_verifier,
                client_salt,
                server_salt,
                server_public_key,
                hex::encode(client_public_key),
            )
        }
    };

    let e = client.verify_server_proof(&server.server_proof());

    let client = match e {
        Ok(s) => s,
        Err(e) => {
            panic!(
                "'{}'\
                \nverifier: {}\
                \nclient_salt: {}\
                \nserver_salt: {}\
                \nserver_public_key: {}\
                \nclient_public_key: {}",
                e,
                password_verifier,
                client_salt,
                server_salt,
                server_public_key,
                hex::encode(client_public_key),
            )
        }
    };

    assert_eq!(*server.session_key(), client.session_key());
    let reconnection_data = client.calculate_reconnect_values(&server.reconnect_challenge_data());

    let verified = server
        .verify_reconnection_attempt(&reconnection_data.challenge_data, &reconnection_data.proof);

    assert!(verified);
}
