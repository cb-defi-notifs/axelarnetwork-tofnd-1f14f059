//! Helper structs and implementations for [crate::gg20::keygen].

use crate::TofndResult;

pub const MAX_PARTY_SHARE_COUNT: usize = tofn::gg20::keygen::MAX_PARTY_SHARE_COUNT;
pub const MAX_TOTAL_SHARE_COUNT: usize = tofn::gg20::keygen::MAX_TOTAL_SHARE_COUNT;

use tracing::{info, span, Level, Span};

/// type for bytes
pub use tofn::sdk::api::BytesVec;

pub enum KeygenOutput {
    Gg20(gg20::TofndKeygenOutput),
    Multisig(multisig::TofndKeygenOutput),
}
pub type TofndKeygenOutput = TofndResult<KeygenOutput>;

/// KeygenInitSanitized holds all arguments needed by Keygen in the desired form; populated by proto::KeygenInit
/// pub because it is also needed by recovery module
pub struct KeygenInitSanitized {
    pub new_key_uid: String,            // session's UID
    pub party_uids: Vec<String>, // vector of party uids; this is alligned with party_share_count vector
    pub party_share_counts: Vec<usize>, // vector of share counts; this is alligned with party_uids vector
    pub my_index: usize, // the _tofnd_ index of the party inside party_uids and party_shares_counts
    pub threshold: usize, // protocol's threshold
}
impl KeygenInitSanitized {
    // get the share count of `my_index`th party
    pub fn my_shares_count(&self) -> usize {
        self.party_share_counts[self.my_index] as usize
    }

    // log KeygenInitSanitized state
    pub fn log_info(&self, keygen_span: Span) {
        // create log span and display current status
        let init_span = span!(parent: &keygen_span, Level::INFO, "init");
        let _enter = init_span.enter();
        info!(
            "[uid:{}, shares:{}] starting Keygen with [key: {}, (t,n)=({},{}), participants:{:?}",
            self.party_uids[self.my_index],
            self.my_shares_count(),
            self.new_key_uid,
            self.threshold,
            self.party_share_counts.iter().sum::<usize>(),
            self.party_uids,
        );
    }
}

/// Context holds the all arguments that need to be passed from keygen gRPC call into protocol execution
#[derive(Clone)]
pub(in super::super) struct Context {
    pub(in super::super) key_id: String, // session id; used for logs
    pub(in super::super) uids: Vec<String>, // all party uids; alligned with `share_counts`
    pub(in super::super) share_counts: Vec<usize>, // all party share counts; alligned with `uids`
    pub(in super::super) threshold: usize, // protocol's threshold
    pub(in super::super) tofnd_index: usize, // tofnd index of party
    pub(in super::super) tofnd_subindex: usize, // share index of party
}

impl Context {
    /// create a new Context
    pub fn new(keygen_init: &KeygenInitSanitized, tofnd_subindex: usize) -> Self {
        Context {
            key_id: keygen_init.new_key_uid.clone(),
            uids: keygen_init.party_uids.clone(),
            share_counts: keygen_init.party_share_counts.clone(),
            threshold: keygen_init.threshold,
            tofnd_index: keygen_init.my_index,
            tofnd_subindex,
        }
    }

    /// export state; used for logging
    pub fn log_info(&self) -> String {
        format!(
            "[{}] [uid:{}, share:{}/{}]",
            self.key_id,
            self.uids[self.tofnd_index],
            self.tofnd_subindex + 1,
            self.share_counts[self.tofnd_index]
        )
    }
}

use crate::grpc::keygen::types::gg20;
use crate::grpc::keygen::types::multisig;
use crate::grpc::service::Service;

pub enum KeygenType {
    Gg20,
    Multisig,
}

pub(in super::super) enum KeygenContext {
    Gg20(gg20::Context),
    Multisig(multisig::Context),
}
use KeygenContext::*;

impl KeygenContext {
    pub async fn new_without_subindex(
        keygen_type: KeygenType,
        service: &Service,
        keygen_init: &KeygenInitSanitized,
    ) -> TofndResult<KeygenContext> {
        let ctx = match keygen_type {
            KeygenType::Gg20 => {
                Gg20(gg20::Context::new_without_subindex(service, keygen_init).await?)
            }
            KeygenType::Multisig => {
                Multisig(multisig::Context::new_without_subindex(service, keygen_init).await?)
            }
        };
        Ok(ctx)
    }

    pub fn clone_with_subindex(&self, tofnd_subindex: usize) -> Self {
        match &self {
            Gg20(gg20_ctx) => Gg20(gg20_ctx.clone_with_subindex(tofnd_subindex)),
            Multisig(multisig_ctx) => Multisig(multisig_ctx.clone_with_subindex(tofnd_subindex)),
        }
    }

    pub fn log_info(&self) -> String {
        match &self {
            Gg20(gg20_ctx) => gg20_ctx.log_info(),
            Multisig(multisig_ctx) => multisig_ctx.log_info(),
        }
    }
}