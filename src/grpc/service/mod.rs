//! This mod includes the service implementation derived from

use super::mnemonic::FileIo;
use super::proto;
use super::types::common::{ServiceKv, DEFAULT_KV_NAME};
use crate::config::Config;
use crate::encrypted_sled::Password;
use std::path::PathBuf;

// error handling
use crate::TofndResult;
use anyhow::anyhow;

#[cfg(feature = "malicious")]
pub mod malicious;

/// Gg20Service
#[derive(Clone)]
pub struct Service {
    pub(super) kv: ServiceKv,
    pub(super) io: FileIo,
    pub(super) cfg: Config,
}

/// create a new Gg20 gRPC server
pub async fn new_service(
    cfg: Config,
    password: Password,
) -> TofndResult<impl proto::gg20_server::Gg20> {
    let kv = ServiceKv::new(&cfg.tofnd_path, DEFAULT_KV_NAME, password)
        .map_err(|err| anyhow!("Shares KV store error: {}", err))?;

    let io = FileIo::new(PathBuf::from(&cfg.tofnd_path));

    let service = Service { kv, io, cfg };

    service.handle_mnemonic().await?;
    Ok(service)
}

pub enum ServiceType {
    Gg20,
    Multisig,
}