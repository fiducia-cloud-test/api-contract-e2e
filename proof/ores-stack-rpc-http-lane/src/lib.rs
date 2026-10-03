#![forbid(unsafe_code)]

use std::path::PathBuf;

pub mod rpc_operation_index {
    #[derive(Clone, Debug, Default)]
    pub struct ServerOperation {
        pub key: String,
        pub stream: String,
        pub codecs: Vec<String>,
        pub default_codec: String,
        pub handlers_source: String,
        pub handler: String,
    }

    #[derive(Clone, Debug, Default)]
    pub struct ServerOperationIndex {
        pub operations: Vec<ServerOperation>,
    }

    impl ServerOperationIndex {
        pub fn parse(_source: &str) -> Result<Self, String> {
            Ok(Self::default())
        }
    }
}

pub mod rpc_route_map {
    pub const GENERATED_SERVER_OPERATION_INDEX: &str =
        "generated/rpc/server-operation-index.json";
}

pub mod rpc_target {
    use super::PathBuf;

    #[derive(Clone, Debug)]
    pub struct RpcTargetSyncOptions {
        pub api_server_repo: PathBuf,
        pub target_repo: PathBuf,
        pub scope: String,
        pub check: bool,
    }
}

mod rpc_http_client_lane;
mod rpc_http_client_lane_post;

pub use rpc_http_client_lane::sync_import_aliases;

pub fn compile_post_adapter(options: &rpc_target::RpcTargetSyncOptions) -> anyhow::Result<()> {
    rpc_http_client_lane_post::materialize_from_canonical_sync(options)
}
