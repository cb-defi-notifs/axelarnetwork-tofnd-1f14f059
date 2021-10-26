use crate::{addr, encrypted_sled::get_test_password, kv_manager::KvManager};
use tokio::{
    self,
    net::TcpListener,
    sync::oneshot::{channel, Sender},
};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Channel;

use super::service::new_service;

use testdir::testdir;
use tracing::error;
use tracing_test::traced_test;

use std::convert::TryInto;

use crate::proto::{
    keygen_response::KeygenResponse, multisig_client::MultisigClient,
    multisig_server::MultisigServer, sign_response::SignResponse, KeygenRequest, SignRequest,
};

// set up tests
async fn spin_test_service_and_client() -> (MultisigClient<Channel>, Sender<()>) {
    // create root directory for service
    let root = testdir!();

    // create a kv_manager
    let kv_manager = KvManager::new(root.to_str().unwrap(), get_test_password())
        .unwrap()
        .handle_mnemonic(&crate::mnemonic::Cmd::Create)
        .await
        .unwrap();

    // create service
    let service = new_service(kv_manager);
    let service = MultisigServer::new(service);

    // create incoming tcp server for service
    let incoming = TcpListener::bind(addr(0)).await.unwrap(); // use port 0 and let the OS decide

    // create shutdown channels
    let (shutdown_sender, shutdown_receiver) = channel::<()>();

    // get server's address
    let server_addr = incoming.local_addr().unwrap();

    // spin up multisig gRPC server with incoming shutdown
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(TcpListenerStream::new(incoming), async {
                shutdown_receiver.await.unwrap();
            })
            .await
            .unwrap();
    });

    // create a client to multisig service
    let client = MultisigClient::connect(format!("http://{}", server_addr))
        .await
        .unwrap();

    // return the client and the shutdown channel for the service
    (client, shutdown_sender)
}

// dummy ctor for KeygenResult
impl KeygenRequest {
    fn new(key_uid: &str) -> KeygenRequest {
        KeygenRequest {
            key_uid: key_uid.to_string(),
            party_uid: String::default(),
        }
    }
}

// dummy ctor for KeygenResult
impl SignRequest {
    fn new(key_uid: &str) -> SignRequest {
        SignRequest {
            key_uid: key_uid.to_string(),
            msg_to_sign: vec![32; 32],
            party_uid: String::default(),
        }
    }
}

// vec to array
fn to_array<T, const N: usize>(v: Vec<T>) -> [T; N] {
    v.try_into()
        .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
}

#[traced_test]
#[tokio::test]
async fn test_multisig() {
    let key = "multisig key";
    let (mut client, shutdown_sender) = spin_test_service_and_client().await;

    let request = KeygenRequest::new(key);

    let response = client.keygen(request).await.unwrap().into_inner();
    let pub_key = match response.keygen_response.unwrap() {
        KeygenResponse::PubKey(pub_key) => pub_key,
        KeygenResponse::Error(err) => {
            panic!("Got error from keygen: {}", err);
        }
    };

    let request = SignRequest::new(key);
    let msg_digest = request.msg_to_sign.as_slice().try_into().unwrap();
    let response = client.sign(request).await.unwrap().into_inner();
    let signature = match response.sign_response.unwrap() {
        SignResponse::Signature(signature) => signature,
        SignResponse::Error(err) => {
            panic!("Got error from sign: {}", err)
        }
    };

    let _ = shutdown_sender.send(()).unwrap();

    assert!(tofn::ecdsa::verify(&to_array(pub_key), &msg_digest, &signature,).unwrap());
}

#[traced_test]
#[tokio::test]
async fn test_tofn_keygen_multisig_fail() {
    let key = "k"; // too small key
    let (mut client, shutdown_sender) = spin_test_service_and_client().await;

    let request = KeygenRequest::new(key);
    let response = client.keygen(request).await.unwrap().into_inner();

    if let KeygenResponse::Error(err) = response.clone().keygen_response.unwrap() {
        error!("{}", err);
    }
    assert!(matches!(
        response.keygen_response.unwrap(),
        KeygenResponse::Error(_)
    ));

    let _ = shutdown_sender.send(()).unwrap();
}

#[traced_test]
#[tokio::test]
async fn test_tofnd_keygen_multisig_fail() {
    let key = "key-uid";
    let (mut client, shutdown_sender) = spin_test_service_and_client().await;

    let request = KeygenRequest::new(key);
    let response = client.keygen(request.clone()).await.unwrap().into_inner();
    assert!(matches!(
        response.keygen_response.unwrap(),
        KeygenResponse::PubKey(_)
    ));

    // try to execute same keygen again
    let response = client.keygen(request).await.unwrap().into_inner();
    if let KeygenResponse::Error(err) = response.clone().keygen_response.unwrap() {
        error!("{}", err);
    }
    assert!(matches!(
        response.keygen_response.unwrap(),
        KeygenResponse::Error(_)
    ));

    let _ = shutdown_sender.send(()).unwrap();
}

#[traced_test]
#[tokio::test]
async fn test_tofn_multisig_sign_fail() {
    let key = "key-uid";
    let (mut client, shutdown_sender) = spin_test_service_and_client().await;

    // keygen should be fine
    let request = KeygenRequest::new(key);
    let response = client.keygen(request.clone()).await.unwrap().into_inner();
    assert!(matches!(
        response.keygen_response.unwrap(),
        KeygenResponse::PubKey(_)
    ));

    // attempt sign with truncated msg digest
    let mut request = SignRequest::new(key);
    request.msg_to_sign = vec![32; 31];
    let response = client.sign(request.clone()).await.unwrap().into_inner();
    if let SignResponse::Error(err) = response.clone().sign_response.unwrap() {
        error!("{}", err);
    }
    assert!(matches!(
        response.sign_response.unwrap(),
        SignResponse::Error(_)
    ));

    // attempt sign with an unkown key
    let request = SignRequest::new("non-existing key");
    let response = client.sign(request.clone()).await.unwrap().into_inner();
    if let SignResponse::Error(err) = response.clone().sign_response.unwrap() {
        error!("{}", err);
    }
    assert!(matches!(
        response.sign_response.unwrap(),
        SignResponse::Error(_)
    ));

    let _ = shutdown_sender.send(()).unwrap();
}

#[traced_test]
#[tokio::test]
async fn test_kv_value_fail() {
    let (mut client, shutdown_sender) = spin_test_service_and_client().await;

    // attempt to get mnemonic value; this should fail because of different value type
    let request = SignRequest::new("mnemonic");
    let response = client.sign(request.clone()).await.unwrap().into_inner();

    if let SignResponse::Error(err) = response.clone().sign_response.unwrap() {
        error!("{}", err);
    }
    assert!(matches!(
        response.sign_response.unwrap(),
        SignResponse::Error(_)
    ));

    let _ = shutdown_sender.send(()).unwrap();
}