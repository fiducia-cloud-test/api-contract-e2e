#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail};
use ores_api_docs::{
    RpcClientAudience, RpcClientBundleV3, RpcClientBundleV3Manifest, RpcClientTransportSourcesV3,
    RpcCodecSet, RpcOperationClientSourcesV3, RpcOperationContract, RpcOperationScope,
    RpcOperationSource, RpcPayloadCodec, RpcRequestShape, RpcResponseShape, RpcStreamMode,
};
use serde_json::Value;
use std::{fs, path::Path};

use crate::{
    rpc_http_client_lane, rpc_operation_index::ServerOperationIndex,
    rpc_route_map::GENERATED_SERVER_OPERATION_INDEX, rpc_target::RpcTargetSyncOptions,
};

const RPC_ENDPOINT: &str = "/v1/rpc";

/// Run after the canonical rpc_target_v3 projection. The parallel lane derives
/// only from retained canonical evidence: the target's api-docs manifest and
/// the source handlers-authoritative operation index. It therefore cannot
/// mutate or reinterpret the canonical generated source path.
pub(crate) fn materialize_from_canonical_sync(options: &RpcTargetSyncOptions) -> Result<()> {
    let source = options
        .api_server_repo
        .canonicalize()
        .with_context(|| format!("resolve API source {}", options.api_server_repo.display()))?;
    let target = options
        .target_repo
        .canonicalize()
        .with_context(|| format!("resolve RPC target {}", options.target_repo.display()))?;
    let scope = options.scope.trim();
    if !matches!(scope, "regular" | "admin") {
        bail!("parallel RPC lane requires regular/admin scope, got {scope:?}");
    }

    let api_docs_manifest_path = target
        .join("generated/rpc")
        .join(scope)
        .join("api-docs-manifest.json");
    let manifest_text = fs::read_to_string(&api_docs_manifest_path).with_context(|| {
        format!(
            "read canonical RPC api-docs manifest {}; canonical sync must finish before the parallel lane",
            api_docs_manifest_path.display()
        )
    })?;
    let manifest_json: Value = serde_json::from_str(&manifest_text)
        .with_context(|| format!("parse {}", api_docs_manifest_path.display()))?;
    let service = required_string(&manifest_json, "service")?.to_owned();
    let audience = required_string(&manifest_json, "audience")?.to_owned();
    let contract_sha256 = required_string(&manifest_json, "contract_sha256")?.to_owned();
    let operations = manifest_json
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("canonical api-docs manifest is missing operations"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow::anyhow!("canonical api-docs operation key must be a string"))
        })
        .collect::<Result<Vec<_>>>()?;

    let index_path = source.join(GENERATED_SERVER_OPERATION_INDEX);
    let index_text = fs::read_to_string(&index_path)
        .with_context(|| format!("read operation index {}", index_path.display()))?;
    let index = ServerOperationIndex::parse(&index_text)
        .map_err(|reason| anyhow::anyhow!("parse {}: {reason}", index_path.display()))?;

    let mut contracts = Vec::with_capacity(operations.len());
    let mut generated_operations = Vec::with_capacity(operations.len());
    for key in &operations {
        let operation = index
            .operations
            .iter()
            .find(|candidate| candidate.key == *key)
            .ok_or_else(|| anyhow::anyhow!(
                "canonical generated operation {key:?} is absent from handlers-authoritative operation index"
            ))?;
        let stream = parse_stream(&operation.stream, key)?;
        let allowed = operation
            .codecs
            .iter()
            .map(|codec| parse_codec(codec, key))
            .collect::<Result<Vec<_>>>()?;
        let default_codec = parse_codec(&operation.default_codec, key)?;
        if !allowed.contains(&default_codec) {
            bail!(
                "operation {key:?} default codec {:?} is not in allowed codecs {:?}",
                operation.default_codec,
                operation.codecs
            );
        }
        let mut key_parts = key.split('.').map(str::to_owned).collect::<Vec<_>>();
        if key_parts.len() < 2 {
            bail!("RPC operation key {key:?} has no service/operation segments");
        }
        key_parts.remove(0);
        key_parts.pop();
        let operation_name = operation.handler.clone();

        contracts.push(RpcOperationContract {
            schema_version: 3,
            operation_key: key.clone(),
            namespace: key_parts.clone(),
            source: RpcOperationSource {
                route_file: None,
                handlers_file: Some(operation.handlers_source.clone()),
                handler: operation.handler.clone(),
                operation: Some(operation_name.clone()),
                invoker: None,
                execution_model: "shared_operation".to_owned(),
                repository: None,
                commit_sha: None,
            },
            rpc_transport_path: RPC_ENDPOINT,
            http: None,
            scope: match scope {
                "regular" => RpcOperationScope::Regular,
                "admin" => RpcOperationScope::Admin,
                _ => unreachable!(),
            },
            stream,
            audiences: vec![match audience.as_str() {
                "public" => RpcClientAudience::Browser,
                "server" => RpcClientAudience::Server,
                other => bail!("unsupported canonical RPC audience {other:?}"),
            }],
            codecs: RpcCodecSet {
                allowed,
                default: default_codec,
            },
            request: RpcRequestShape::default(),
            response: RpcResponseShape::default(),
            contract_sha256: contract_sha256.clone(),
        });
        generated_operations.push(RpcOperationClientSourcesV3 {
            operation_key: key.clone(),
            namespace: key_parts,
            operation_name,
            stream,
            rust: String::new(),
            go: String::new(),
            dart: String::new(),
            typescript: String::new(),
            gleam: String::new(),
        });
    }

    let bundle = RpcClientBundleV3 {
        manifest: RpcClientBundleV3Manifest {
            schema_version: 3,
            generated_by: "ores-stack parallel lane evidence adapter",
            service,
            audience,
            contract_sha256,
            operations,
            languages: ["rust", "go", "dart", "typescript", "gleam"],
            http_endpoint: RPC_ENDPOINT,
            layout: "namespace-files/v1",
        },
        transport: RpcClientTransportSourcesV3 {
            rust: String::new(),
            go: String::new(),
            dart: String::new(),
            typescript: String::new(),
            gleam: String::new(),
        },
        operations: generated_operations,
    };

    rpc_http_client_lane::materialize(&target, &bundle, &contracts, scope, options.check)
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("canonical api-docs manifest is missing non-empty {key:?}"))
}

fn parse_stream(value: &str, key: &str) -> Result<RpcStreamMode> {
    match value {
        "unary" => Ok(RpcStreamMode::Unary),
        "server_stream" => Ok(RpcStreamMode::ServerStream),
        "client_stream" => Ok(RpcStreamMode::ClientStream),
        "bidi" => Ok(RpcStreamMode::Bidi),
        other => bail!("unsupported stream mode {other:?} for {key:?}"),
    }
}

fn parse_codec(value: &str, key: &str) -> Result<RpcPayloadCodec> {
    match value {
        "json" => Ok(RpcPayloadCodec::Json),
        "messagepack" => Ok(RpcPayloadCodec::Messagepack),
        "protobuf" => Ok(RpcPayloadCodec::Protobuf),
        other => bail!("unsupported codec {other:?} for {key:?}"),
    }
}
