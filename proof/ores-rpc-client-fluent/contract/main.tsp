/**
 * Fleet-generic semantic contract for generated RPC client calls.
 *
 * This contract describes the stable operation/call capability shared by
 * generated clients and transport runtimes. It deliberately does not prescribe
 * inheritance: TypeScript/Dart may subclass a runtime base while Rust/Gleam may
 * use traits/records/composition.
 *
 * This TypeSpec source and authored.schema.json are independent peer
 * authorities. Neither may be generated from the other to force parity.
 */
namespace Ores.Interfaces.RpcClientCall.V1;

/** Canonical lowercase HTTP header names mapped to unfolded string values. */
model HeaderMap is Record<string>;

/** Payload codecs admitted by the shared RPC transport. */
enum RpcPayloadCodec {
  json: "json",
  messagepack: "messagepack",
  protobuf: "protobuf",
}

/** Handlers-authoritative response shape for a generated operation. */
enum RpcStreamMode {
  unary: "unary",
  server_stream: "server_stream",
}

/** Terminal action selected after fluent request configuration. */
enum RpcCallTerminal {
  make_call: "make_call",
  do_stream: "do_stream",
}

/** Stable, product-neutral descriptor embedded by generated client code. */
model RpcOperationDescriptor {
  operation_key: string;
  endpoint: string;
  stream_mode: RpcStreamMode;
  allowed_codecs: RpcPayloadCodec[];
  default_codec: RpcPayloadCodec;
  contract_sha256: string;
}

/**
 * Immutable call options accumulated by a fluent request builder before I/O.
 * Cancellation handles/signals are runtime objects and therefore intentionally
 * stay outside this JSON-value contract while remaining part of language SDK
 * capability interfaces.
 */
model RpcCallOptions {
  headers: HeaderMap;
  timeout_ms?: uint32;
  codec?: RpcPayloadCodec;
}

/** Auditable semantic intent at the execution boundary. */
model RpcCallIntent {
  operation: RpcOperationDescriptor;
  options: RpcCallOptions;
  terminal: RpcCallTerminal;
}
