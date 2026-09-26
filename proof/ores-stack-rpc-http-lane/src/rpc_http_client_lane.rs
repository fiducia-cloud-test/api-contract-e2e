#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail};
use ores_api_docs::{
    RpcClientBundleV3, RpcOperationContract, RpcPayloadCodec, RpcSdkLanguage, RpcStreamMode,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const LANE_SCHEMA: &str = "ores.stack.rpc-http-client-lane/v1";
const IMPORT_SCHEMA: &str = "ores.stack.rpc-client-imports/v1";
const INTERFACE_CONTRACT: &str = "ORESoftware/ores-interfaces/contracts/rpc-client-call/v1";
const GENERATOR: &str = "ores-stack sync";
const LANE_DIR: &str = "generated_ores_http";

#[derive(Debug, Serialize)]
struct LaneManifest {
    schema: &'static str,
    generated_by: &'static str,
    interface_contract: &'static str,
    scope: String,
    service: String,
    audience: String,
    contract_sha256: String,
    operation_keys: Vec<String>,
    files: Vec<String>,
    runtime_packages: RuntimePackages,
}

#[derive(Debug, Serialize)]
struct RuntimePackages {
    typescript: &'static str,
    dart: &'static str,
    rust: &'static str,
    gleam: &'static str,
}

#[derive(Debug, Serialize)]
struct ImportLane {
    typescript: String,
    dart: String,
    rust: String,
    gleam: String,
}

#[derive(Debug, Serialize)]
struct ImportManifest {
    schema: &'static str,
    generated_by: &'static str,
    interface_contract: &'static str,
    canonical: ImportLane,
    ores_http_clients: ImportLane,
    runtime_packages: RuntimePackages,
}

fn runtime_packages() -> RuntimePackages {
    RuntimePackages {
        typescript: "@oresoftware/ores-http-clients/fluent",
        dart: "package:ores_http_clients/ores_http_clients.dart",
        rust: "ores_http_clients",
        gleam: "ores_http_clients/fluent",
    }
}

/// Materialize an additive client lane that targets `ores-http-clients` fluent
/// builders without touching the canonical api-docs-generated modules.
///
/// The canonical lane remains authoritative during this migration. This lane
/// imports its DTOs from that canonical lane and changes only the client/runtime
/// binding. TypeScript/Dart use base classes; Rust uses composition; Gleam uses
/// pipeline-friendly builder functions.
pub(crate) fn materialize(
    target: &Path,
    bundle: &RpcClientBundleV3,
    contracts: &[RpcOperationContract],
    scope: &str,
    check: bool,
) -> Result<()> {
    let by_key = contracts
        .iter()
        .map(|contract| (contract.operation_key.as_str(), contract))
        .collect::<BTreeMap<_, _>>();

    let mut expected = BTreeMap::<PathBuf, String>::new();
    let mut operation_keys = Vec::with_capacity(bundle.operations.len());
    for operation in &bundle.operations {
        let contract = by_key
            .get(operation.operation_key.as_str())
            .ok_or_else(|| anyhow::anyhow!(
                "parallel ores-http-clients lane has no contract for {:?}",
                operation.operation_key
            ))?;
        if contract.stream != operation.stream {
            bail!(
                "parallel ores-http-clients lane stream mode drift for {:?}: contract {:?} != bundle {:?}",
                operation.operation_key,
                contract.stream,
                operation.stream
            );
        }
        if !matches!(operation.stream, RpcStreamMode::Unary | RpcStreamMode::ServerStream) {
            bail!(
                "parallel ores-http-clients lane does not support {:?} for {:?}",
                operation.stream,
                operation.operation_key
            );
        }
        operation_keys.push(operation.operation_key.clone());
        let operation_name = &operation.operation_name;
        let pascal = pascal(operation_name);

        for language in [
            RpcSdkLanguage::TypeScript,
            RpcSdkLanguage::Dart,
            RpcSdkLanguage::Rust,
            RpcSdkLanguage::Gleam,
        ] {
            let relative = lane_operation_path(language, &operation.namespace, operation_name);
            let source = match language {
                RpcSdkLanguage::TypeScript => typescript_source(
                    &operation.namespace,
                    operation_name,
                    &pascal,
                    contract,
                    &bundle.manifest.contract_sha256,
                ),
                RpcSdkLanguage::Dart => dart_source(
                    &operation.namespace,
                    operation_name,
                    &pascal,
                    contract,
                    &bundle.manifest.contract_sha256,
                ),
                RpcSdkLanguage::Rust => rust_source(
                    &operation.namespace,
                    operation_name,
                    &pascal,
                    contract,
                    &bundle.manifest.contract_sha256,
                ),
                RpcSdkLanguage::Gleam => gleam_source(
                    &operation.namespace,
                    operation_name,
                    &pascal,
                    contract,
                    &bundle.manifest.contract_sha256,
                ),
                RpcSdkLanguage::Go => unreachable!("parallel fluent lane is four-language only"),
            }?;
            let content = format!(
                "{}{}",
                lane_header(language, &operation.operation_key),
                source
            );
            if expected.insert(relative.clone(), content).is_some() {
                bail!("duplicate parallel RPC lane path {}", relative.display());
            }
        }
    }
    operation_keys.sort();

    add_rust_indexes(&mut expected, &operation_keys)?;
    let files = expected
        .keys()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();

    let scope_root = target.join("generated/rpc").join(scope);
    let manifest_path = scope_root.join("ores-http-clients-manifest.json");
    reconcile_previous_files(target, &manifest_path, &expected, check)?;
    for (relative, content) in &expected {
        write_or_check(target, &target.join(relative), content, check)?;
    }

    let manifest = LaneManifest {
        schema: LANE_SCHEMA,
        generated_by: GENERATOR,
        interface_contract: INTERFACE_CONTRACT,
        scope: scope.to_owned(),
        service: bundle.manifest.service.clone(),
        audience: bundle.manifest.audience.clone(),
        contract_sha256: bundle.manifest.contract_sha256.clone(),
        operation_keys,
        files,
        runtime_packages: runtime_packages(),
    };
    let manifest_text = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    write_or_check(target, &manifest_path, &manifest_text, check)?;
    sync_import_aliases(target, check)?;
    Ok(())
}

/// Build/check-time projection of the two import roots. This function has no
/// dependency on API-server source and therefore can run during `ores-stack
/// build` after `sync` has materialized an alternate lane.
///
/// Returns false when the repository has never opted into the alternate lane.
pub fn sync_import_aliases(target: &Path, check: bool) -> Result<bool> {
    let generated_rpc = target.join("generated/rpc");
    let enabled = ["regular", "admin"].iter().any(|scope| {
        generated_rpc
            .join(scope)
            .join("ores-http-clients-manifest.json")
            .is_file()
    });
    if !enabled {
        return Ok(false);
    }
    let manifest = ImportManifest {
        schema: IMPORT_SCHEMA,
        generated_by: "ores-stack build/sync",
        interface_contract: INTERFACE_CONTRACT,
        canonical: ImportLane {
            typescript: "src/langs/typescript/generated".into(),
            dart: "src/langs/dart/generated".into(),
            rust: "src/langs/rust/generated".into(),
            gleam: "src/langs/gleam/generated".into(),
        },
        ores_http_clients: ImportLane {
            typescript: "src/langs/typescript/generated_ores_http".into(),
            dart: "src/langs/dart/generated_ores_http".into(),
            rust: "src/langs/rust/generated_ores_http".into(),
            gleam: "src/langs/gleam/generated_ores_http".into(),
        },
        runtime_packages: runtime_packages(),
    };
    let path = generated_rpc.join("client-imports.json");
    let content = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    write_or_check(target, &path, &content, check)?;
    Ok(true)
}

fn lane_operation_path(
    language: RpcSdkLanguage,
    namespace: &[String],
    operation_name: &str,
) -> PathBuf {
    let mut path = PathBuf::from("src")
        .join("langs")
        .join(language.directory_name())
        .join(LANE_DIR);
    for segment in namespace {
        path.push(segment);
    }
    let stem = match language {
        RpcSdkLanguage::Gleam => operation_name.to_owned(),
        _ => operation_name.replace('_', "-"),
    };
    path.push(format!("{stem}.{}", language.extension()));
    path
}

fn canonical_relative_import(
    language: RpcSdkLanguage,
    namespace: &[String],
    operation_name: &str,
) -> String {
    let ups = "../".repeat(namespace.len() + 1);
    let namespace_path = if namespace.is_empty() {
        String::new()
    } else {
        format!("{}/", namespace.join("/"))
    };
    let stem = match language {
        RpcSdkLanguage::Gleam => operation_name.to_owned(),
        _ => operation_name.replace('_', "-"),
    };
    format!("{ups}generated/{namespace_path}{stem}")
}

fn typescript_source(
    namespace: &[String],
    operation_name: &str,
    pascal: &str,
    contract: &RpcOperationContract,
    digest: &str,
) -> Result<String> {
    let canonical = canonical_relative_import(
        RpcSdkLanguage::TypeScript,
        namespace,
        operation_name,
    );
    let descriptor = ts_descriptor(contract, digest);
    let class_name = format!("{pascal}OresHttpClient");
    let camel = camel(operation_name);
    Ok(match contract.stream {
        RpcStreamMode::Unary => format!(
            "import {{ OresRpcClientBase, type RpcOperationDescriptor, type RpcUnaryCallBuilder, type RpcUnaryExecutor }} from \"@oresoftware/ores-http-clients/fluent\";\nimport type {{ {pascal}Input, {pascal}Response }} from \"{canonical}.js\";\n\nconst OPERATION = {descriptor} satisfies RpcOperationDescriptor;\n\nexport class {class_name} extends OresRpcClientBase {{\n  constructor(private readonly execute: RpcUnaryExecutor<{pascal}Input, {pascal}Response>) {{ super(); }}\n\n  {camel}(input: {pascal}Input): RpcUnaryCallBuilder<{pascal}Input, {pascal}Response> {{\n    return this.unary(OPERATION, input, this.execute);\n  }}\n}}\n"
        ),
        RpcStreamMode::ServerStream => format!(
            "import {{ OresRpcClientBase, type RpcOperationDescriptor, type RpcServerStreamCallBuilder, type RpcStreamExecutor }} from \"@oresoftware/ores-http-clients/fluent\";\nimport type {{ {pascal}Input, {pascal}Response }} from \"{canonical}.js\";\n\nconst OPERATION = {descriptor} satisfies RpcOperationDescriptor;\n\nexport class {class_name} extends OresRpcClientBase {{\n  constructor(private readonly open: RpcStreamExecutor<{pascal}Input, {pascal}Response>) {{ super(); }}\n\n  {camel}(input: {pascal}Input): RpcServerStreamCallBuilder<{pascal}Input, {pascal}Response> {{\n    return this.serverStream(OPERATION, input, this.open);\n  }}\n}}\n"
        ),
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    })
}

fn dart_source(
    namespace: &[String],
    operation_name: &str,
    pascal: &str,
    contract: &RpcOperationContract,
    digest: &str,
) -> Result<String> {
    let canonical = canonical_relative_import(RpcSdkLanguage::Dart, namespace, operation_name);
    let class_name = format!("{pascal}OresHttpClient");
    let camel = camel(operation_name);
    let descriptor = dart_descriptor(contract, digest);
    Ok(match contract.stream {
        RpcStreamMode::Unary => format!(
            "import 'package:ores_http_clients/ores_http_clients.dart';\nimport '{canonical}.dart';\n\nfinal _operation = {descriptor};\n\nfinal class {class_name} extends OresRpcClientBase {{\n  {class_name}(this._execute);\n  final RpcUnaryExecutor<{pascal}Input, {pascal}Response> _execute;\n\n  RpcUnaryCallBuilder<{pascal}Input, {pascal}Response> {camel}({pascal}Input input) =>\n      unary(_operation, input, _execute);\n}}\n"
        ),
        RpcStreamMode::ServerStream => format!(
            "import 'package:ores_http_clients/ores_http_clients.dart';\nimport '{canonical}.dart';\n\nfinal _operation = {descriptor};\n\nfinal class {class_name} extends OresRpcClientBase {{\n  {class_name}(this._open);\n  final RpcStreamExecutor<{pascal}Input, {pascal}Response> _open;\n\n  RpcServerStreamCallBuilder<{pascal}Input, {pascal}Response> {camel}({pascal}Input input) =>\n      serverStream(_operation, input, _open);\n}}\n"
        ),
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    })
}

fn rust_source(
    namespace: &[String],
    operation_name: &str,
    pascal: &str,
    contract: &RpcOperationContract,
    digest: &str,
) -> Result<String> {
    let module_path = if namespace.is_empty() {
        format!("crate::generated::{}", rust_ident(operation_name))
    } else {
        format!(
            "crate::generated::{}::{}",
            namespace
                .iter()
                .map(|segment| rust_ident(segment))
                .collect::<Vec<_>>()
                .join("::"),
            rust_ident(operation_name)
        )
    };
    let descriptor = rust_descriptor(contract, digest);
    let class_name = format!("{pascal}OresHttpClient");
    Ok(match contract.stream {
        RpcStreamMode::Unary => format!(
            "use ::ores_http_clients::{{OresRpcClientBase, RpcOperationDescriptor, RpcUnaryCallBuilder, RpcUnaryExecutor}};\nuse {module_path}::{{{pascal}Input, {pascal}Response}};\n\nconst OPERATION: RpcOperationDescriptor = {descriptor};\n\npub struct {class_name}<E> {{\n    base: OresRpcClientBase,\n    execute: RpcUnaryExecutor<{pascal}Input, {pascal}Response, E>,\n}}\n\nimpl<E> {class_name}<E> {{\n    pub fn new(execute: RpcUnaryExecutor<{pascal}Input, {pascal}Response, E>) -> Self {{\n        Self {{ base: OresRpcClientBase, execute }}\n    }}\n\n    pub fn {operation_name}(&self, input: {pascal}Input) -> RpcUnaryCallBuilder<{pascal}Input, {pascal}Response, E> {{\n        self.base.unary(OPERATION, input, ::std::sync::Arc::clone(&self.execute))\n    }}\n}}\n"
        ),
        RpcStreamMode::ServerStream => format!(
            "use ::ores_http_clients::{{OresRpcClientBase, RpcOperationDescriptor, RpcServerStreamCallBuilder, RpcStreamExecutor}};\nuse {module_path}::{{{pascal}Input, {pascal}Response}};\n\nconst OPERATION: RpcOperationDescriptor = {descriptor};\n\npub struct {class_name}<E> {{\n    base: OresRpcClientBase,\n    open: RpcStreamExecutor<{pascal}Input, {pascal}Response, E>,\n}}\n\nimpl<E> {class_name}<E> {{\n    pub fn new(open: RpcStreamExecutor<{pascal}Input, {pascal}Response, E>) -> Self {{\n        Self {{ base: OresRpcClientBase, open }}\n    }}\n\n    pub fn {operation_name}(&self, input: {pascal}Input) -> RpcServerStreamCallBuilder<{pascal}Input, {pascal}Response, E> {{\n        self.base.server_stream(OPERATION, input, ::std::sync::Arc::clone(&self.open))\n    }}\n}}\n"
        ),
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    })
}

fn gleam_source(
    namespace: &[String],
    operation_name: &str,
    pascal: &str,
    contract: &RpcOperationContract,
    digest: &str,
) -> Result<String> {
    let canonical_module = if namespace.is_empty() {
        format!("langs/gleam/generated/{operation_name}")
    } else {
        format!(
            "langs/gleam/generated/{}/{}",
            namespace.join("/"),
            operation_name
        )
    };
    let descriptor = gleam_descriptor(contract, digest);
    Ok(match contract.stream {
        RpcStreamMode::Unary => format!(
            "import ores_http_clients/fluent\nimport {canonical_module} as canonical\n\npub fn operation_descriptor() -> fluent.RpcOperationDescriptor {{\n  {descriptor}\n}}\n\npub fn {operation_name}(execute: fn(fluent.RpcOperationDescriptor, canonical.{pascal}Input, fluent.RpcCallOptions) -> Result(canonical.{pascal}Response, error), input: canonical.{pascal}Input) -> fluent.RpcUnaryCallBuilder(canonical.{pascal}Input, canonical.{pascal}Response, error) {{\n  fluent.unary(operation_descriptor(), input, execute)\n}}\n"
        ),
        RpcStreamMode::ServerStream => format!(
            "import ores_http_clients/fluent\nimport {canonical_module} as canonical\n\npub fn operation_descriptor() -> fluent.RpcOperationDescriptor {{\n  {descriptor}\n}}\n\npub fn {operation_name}(open: fn(fluent.RpcOperationDescriptor, canonical.{pascal}Input, fluent.RpcCallOptions) -> Result(stream, error), input: canonical.{pascal}Input) -> fluent.RpcServerStreamCallBuilder(canonical.{pascal}Input, stream, error) {{\n  fluent.server_stream(operation_descriptor(), input, open)\n}}\n"
        ),
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    })
}

fn ts_descriptor(contract: &RpcOperationContract, digest: &str) -> String {
    format!(
        "{{ operationKey: {:?}, endpoint: {:?}, streamMode: {:?}, allowedCodecs: [{}], defaultCodec: {:?}, contractSha256: {:?} }} as const",
        contract.operation_key,
        contract.rpc_transport_path,
        contract.stream.as_str(),
        contract
            .codecs
            .allowed
            .iter()
            .map(|codec| format!("{:?}", codec.as_str()))
            .collect::<Vec<_>>()
            .join(", "),
        contract.codecs.default.as_str(),
        digest,
    )
}

fn dart_descriptor(contract: &RpcOperationContract, digest: &str) -> String {
    format!(
        "RpcOperationDescriptor(operationKey: {:?}, endpoint: {:?}, streamMode: RpcGeneratedStreamMode.{}, allowedCodecs: const [{}], defaultCodec: RpcPayloadCodecName.{}, contractSha256: {:?})",
        contract.operation_key,
        contract.rpc_transport_path,
        dart_stream(contract.stream),
        contract
            .codecs
            .allowed
            .iter()
            .map(|codec| format!("RpcPayloadCodecName.{}", dart_codec(*codec)))
            .collect::<Vec<_>>()
            .join(", "),
        dart_codec(contract.codecs.default),
        digest,
    )
}

fn rust_descriptor(contract: &RpcOperationContract, digest: &str) -> String {
    format!(
        "RpcOperationDescriptor {{ operation_key: {:?}, endpoint: {:?}, stream_mode: ::ores_http_clients::RpcGeneratedStreamMode::{}, allowed_codecs: &[{}], default_codec: ::ores_http_clients::RpcPayloadCodecName::{}, contract_sha256: {:?} }}",
        contract.operation_key,
        contract.rpc_transport_path,
        rust_stream(contract.stream),
        contract
            .codecs
            .allowed
            .iter()
            .map(|codec| format!("::ores_http_clients::RpcPayloadCodecName::{}", rust_codec(*codec)))
            .collect::<Vec<_>>()
            .join(", "),
        rust_codec(contract.codecs.default),
        digest,
    )
}

fn gleam_descriptor(contract: &RpcOperationContract, digest: &str) -> String {
    format!(
        "fluent.RpcOperationDescriptor({:?}, {:?}, fluent.{}, [{}], fluent.{}, {:?})",
        contract.operation_key,
        contract.rpc_transport_path,
        gleam_stream(contract.stream),
        contract
            .codecs
            .allowed
            .iter()
            .map(|codec| format!("fluent.{}", gleam_codec(*codec)))
            .collect::<Vec<_>>()
            .join(", "),
        gleam_codec(contract.codecs.default),
        digest,
    )
}

fn dart_codec(codec: RpcPayloadCodec) -> &'static str {
    match codec {
        RpcPayloadCodec::Json => "json",
        RpcPayloadCodec::Messagepack => "messagepack",
        RpcPayloadCodec::Protobuf => "protobuf",
    }
}

fn rust_codec(codec: RpcPayloadCodec) -> &'static str {
    match codec {
        RpcPayloadCodec::Json => "Json",
        RpcPayloadCodec::Messagepack => "Messagepack",
        RpcPayloadCodec::Protobuf => "Protobuf",
    }
}

fn gleam_codec(codec: RpcPayloadCodec) -> &'static str {
    match codec {
        RpcPayloadCodec::Json => "Json",
        RpcPayloadCodec::Messagepack => "Messagepack",
        RpcPayloadCodec::Protobuf => "Protobuf",
    }
}

fn dart_stream(mode: RpcStreamMode) -> &'static str {
    match mode {
        RpcStreamMode::Unary => "unary",
        RpcStreamMode::ServerStream => "serverStream",
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    }
}

fn rust_stream(mode: RpcStreamMode) -> &'static str {
    match mode {
        RpcStreamMode::Unary => "Unary",
        RpcStreamMode::ServerStream => "ServerStream",
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    }
}

fn gleam_stream(mode: RpcStreamMode) -> &'static str {
    match mode {
        RpcStreamMode::Unary => "Unary",
        RpcStreamMode::ServerStream => "ServerStream",
        RpcStreamMode::ClientStream | RpcStreamMode::Bidi => unreachable!(),
    }
}

fn lane_header(language: RpcSdkLanguage, key: &str) -> String {
    match language {
        RpcSdkLanguage::TypeScript => format!(
            "/** @generated by ores-stack parallel ores-http-clients lane from {key}; DO NOT EDIT. */\n"
        ),
        RpcSdkLanguage::Dart | RpcSdkLanguage::Rust | RpcSdkLanguage::Gleam => format!(
            "// @generated by ores-stack parallel ores-http-clients lane from {key}; DO NOT EDIT.\n"
        ),
        RpcSdkLanguage::Go => unreachable!(),
    }
}

fn add_rust_indexes(
    expected: &mut BTreeMap<PathBuf, String>,
    operation_keys: &[String],
) -> Result<()> {
    #[derive(Default)]
    struct Node {
        children: BTreeSet<String>,
        operations: Vec<String>,
    }
    let mut nodes = BTreeMap::<Vec<String>, Node>::new();
    nodes.entry(Vec::new()).or_default();
    for key in operation_keys {
        let mut parts = key.split('.').map(str::to_owned).collect::<Vec<_>>();
        if parts.len() < 2 {
            bail!("invalid RPC key {key:?} in parallel lane");
        }
        parts.remove(0);
        let operation = parts.pop().expect("operation tail");
        for depth in 0..=parts.len() {
            let prefix = parts[..depth].to_vec();
            let node = nodes.entry(prefix).or_default();
            if depth < parts.len() {
                node.children.insert(parts[depth].clone());
            } else {
                node.operations.push(operation.clone());
            }
        }
    }
    for (namespace, mut node) in nodes {
        node.operations.sort();
        node.operations.dedup();
        let mut content = String::from("// @generated by ores-stack parallel ores-http-clients lane; DO NOT EDIT.\n");
        for child in node.children {
            content.push_str(&format!("pub mod {};\n", rust_ident(&child)));
        }
        for operation in node.operations {
            content.push_str(&format!(
                "#[path = {:?}]\npub mod {};\n",
                format!("{}.rs", operation.replace('_', "-")),
                rust_ident(&operation)
            ));
        }
        let mut path = PathBuf::from("src/langs/rust").join(LANE_DIR);
        for segment in namespace {
            path.push(segment);
        }
        path.push("mod.rs");
        expected.insert(path, content);
    }
    Ok(())
}

fn rust_ident(value: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "try",
    ];
    if KEYWORDS.contains(&value) {
        format!("r#{value}")
    } else {
        value.to_owned()
    }
}

fn pascal(value: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper {
                out.extend(ch.to_uppercase());
                upper = false;
            } else {
                out.push(ch);
            }
        } else {
            upper = true;
        }
    }
    out
}

fn camel(value: &str) -> String {
    let pascal = pascal(value);
    let mut chars = pascal.chars();
    let Some(first) = chars.next() else {
        return pascal;
    };
    format!("{}{}", first.to_ascii_lowercase(), chars.collect::<String>())
}

fn reconcile_previous_files(
    target: &Path,
    manifest_path: &Path,
    expected: &BTreeMap<PathBuf, String>,
    check: bool,
) -> Result<()> {
    let source = match fs::read_to_string(manifest_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("read {}", manifest_path.display())),
    };
    let value: serde_json::Value = serde_json::from_str(&source)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    if value.get("schema").and_then(serde_json::Value::as_str) != Some(LANE_SCHEMA) {
        bail!(
            "prior parallel lane manifest {} has unknown schema; refusing cleanup",
            manifest_path.display()
        );
    }
    let prior = value
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("prior parallel lane manifest has no files list"))?;
    for relative in prior.iter().filter_map(serde_json::Value::as_str) {
        let relative_path = PathBuf::from(relative);
        if expected.contains_key(&relative_path) {
            continue;
        }
        if !relative_path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("unsafe prior parallel lane path {relative:?}");
        }
        let path = target.join(&relative_path);
        if !path.exists() {
            continue;
        }
        if check {
            bail!(
                "stale parallel ores-http-clients RPC file {}; run `ores-stack sync`",
                path.display()
            );
        }
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("inspect stale parallel RPC file {}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!(
                "stale parallel RPC path {} is not a regular file; refusing removal",
                path.display()
            );
        }
        fs::remove_file(&path)
            .with_context(|| format!("remove stale parallel RPC file {}", path.display()))?;
    }
    Ok(())
}

fn write_or_check(target: &Path, path: &Path, content: &str, check: bool) -> Result<()> {
    if !path.starts_with(target) {
        bail!("parallel RPC output {} escapes target {}", path.display(), target.display());
    }
    if check {
        let current = fs::read_to_string(path)
            .with_context(|| format!("parallel RPC generated file missing: {}", path.display()))?;
        if current != content {
            bail!(
                "parallel ores-http-clients RPC drift at {}; run `ores-stack sync`",
                path.display()
            );
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parallel RPC directory {}", parent.display()))?;
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("parallel RPC output {} must be a regular file", path.display());
        }
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternate_path_is_sibling_of_canonical_generated_root() {
        let path = lane_operation_path(
            RpcSdkLanguage::TypeScript,
            &["admin".into(), "version".into()],
            "get_version",
        );
        assert_eq!(
            path,
            PathBuf::from("src/langs/typescript/generated_ores_http/admin/version/get-version.ts")
        );
        assert_eq!(
            canonical_relative_import(
                RpcSdkLanguage::TypeScript,
                &["admin".into(), "version".into()],
                "get_version"
            ),
            "../../../generated/admin/version/get-version"
        );
    }

    #[test]
    fn import_manifest_names_both_lanes_and_shared_contract() {
        let manifest = ImportManifest {
            schema: IMPORT_SCHEMA,
            generated_by: "test",
            interface_contract: INTERFACE_CONTRACT,
            canonical: ImportLane {
                typescript: "src/langs/typescript/generated".into(),
                dart: "src/langs/dart/generated".into(),
                rust: "src/langs/rust/generated".into(),
                gleam: "src/langs/gleam/generated".into(),
            },
            ores_http_clients: ImportLane {
                typescript: "src/langs/typescript/generated_ores_http".into(),
                dart: "src/langs/dart/generated_ores_http".into(),
                rust: "src/langs/rust/generated_ores_http".into(),
                gleam: "src/langs/gleam/generated_ores_http".into(),
            },
            runtime_packages: runtime_packages(),
        };
        let json = serde_json::to_string(&manifest).expect("manifest");
        assert!(json.contains("generated_ores_http"));
        assert!(json.contains("@oresoftware/ores-http-clients/fluent"));
        assert!(json.contains(INTERFACE_CONTRACT));
    }
}
