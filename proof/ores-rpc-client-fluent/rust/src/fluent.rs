use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};
use futures_core::Stream;

#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RpcPayloadCodecName { Json, Messagepack, Protobuf }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RpcGeneratedStreamMode { Unary, ServerStream }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct RpcOperationDescriptor { pub operation_key: &'static str, pub endpoint: &'static str, pub stream_mode: RpcGeneratedStreamMode, pub allowed_codecs: &'static [RpcPayloadCodecName], pub default_codec: RpcPayloadCodecName, pub contract_sha256: &'static str }
#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct RpcCallOptions { pub headers: BTreeMap<String, String>, pub timeout_ms: Option<u32>, pub codec: Option<RpcPayloadCodecName> }
pub type BoxRpcFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'static>>;
pub type BoxRpcStream<T, E> = Pin<Box<dyn Stream<Item = Result<T, E>> + Send + 'static>>;
pub type RpcUnaryExecutor<I, O, E> = Arc<dyn Fn(RpcOperationDescriptor, I, RpcCallOptions) -> BoxRpcFuture<O, E> + Send + Sync>;
pub type RpcStreamExecutor<I, O, E> = Arc<dyn Fn(RpcOperationDescriptor, I, RpcCallOptions) -> BoxRpcFuture<BoxRpcStream<O, E>, E> + Send + Sync>;

fn normalize_header_name(name: impl Into<String>) -> String { let normalized = name.into().trim().to_ascii_lowercase(); assert!(!normalized.is_empty(), "RPC header name must be non-empty"); assert!(normalized.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~')), "invalid RPC header name: {normalized:?}"); normalized }
fn assert_codec_allowed(operation: RpcOperationDescriptor, codec: RpcPayloadCodecName) { assert!(operation.allowed_codecs.contains(&codec), "RPC {} does not admit codec {:?}", operation.operation_key, codec); }

#[derive(Clone)] pub struct RpcUnaryCallBuilder<I, O, E> { pub operation: RpcOperationDescriptor, input: I, pub options: RpcCallOptions, execute: RpcUnaryExecutor<I, O, E> }
impl<I, O, E> RpcUnaryCallBuilder<I, O, E> {
  #[must_use] pub fn new(operation: RpcOperationDescriptor, input: I, execute: RpcUnaryExecutor<I, O, E>) -> Self { assert_eq!(operation.stream_mode, RpcGeneratedStreamMode::Unary, "unary builder requires a unary operation descriptor"); Self { operation, input, options: RpcCallOptions::default(), execute } }
  #[must_use] pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self { self.options.headers.insert(normalize_header_name(name), value.into()); self }
  #[must_use] pub fn headers(mut self, values: impl IntoIterator<Item = (String, String)>) -> Self { for (name, value) in values { self.options.headers.insert(normalize_header_name(name), value); } self }
  #[must_use] pub fn timeout(mut self, milliseconds: u32) -> Self { self.options.timeout_ms = Some(milliseconds); self }
  #[must_use] pub fn codec(mut self, codec: RpcPayloadCodecName) -> Self { assert_codec_allowed(self.operation, codec); self.options.codec = Some(codec); self }
  pub fn make_call(self) -> BoxRpcFuture<O, E> { (self.execute)(self.operation, self.input, self.options) }
  #[allow(non_snake_case)] pub fn makeCall(self) -> BoxRpcFuture<O, E> { self.make_call() }
}

#[derive(Clone)] pub struct RpcServerStreamCallBuilder<I, O, E> { pub operation: RpcOperationDescriptor, input: I, pub options: RpcCallOptions, open: RpcStreamExecutor<I, O, E> }
impl<I, O, E> RpcServerStreamCallBuilder<I, O, E> {
  #[must_use] pub fn new(operation: RpcOperationDescriptor, input: I, open: RpcStreamExecutor<I, O, E>) -> Self { assert_eq!(operation.stream_mode, RpcGeneratedStreamMode::ServerStream, "stream builder requires a server_stream operation descriptor"); Self { operation, input, options: RpcCallOptions::default(), open } }
  #[must_use] pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self { self.options.headers.insert(normalize_header_name(name), value.into()); self }
  #[must_use] pub fn headers(mut self, values: impl IntoIterator<Item = (String, String)>) -> Self { for (name, value) in values { self.options.headers.insert(normalize_header_name(name), value); } self }
  #[must_use] pub fn timeout(mut self, milliseconds: u32) -> Self { self.options.timeout_ms = Some(milliseconds); self }
  #[must_use] pub fn codec(mut self, codec: RpcPayloadCodecName) -> Self { assert_codec_allowed(self.operation, codec); self.options.codec = Some(codec); self }
  pub fn do_stream(self) -> BoxRpcFuture<BoxRpcStream<O, E>, E> { (self.open)(self.operation, self.input, self.options) }
  #[allow(non_snake_case)] pub fn doStream(self) -> BoxRpcFuture<BoxRpcStream<O, E>, E> { self.do_stream() }
}

#[derive(Clone, Copy, Debug, Default)] pub struct OresRpcClientBase;
impl OresRpcClientBase {
  #[must_use] pub fn unary<I, O, E>(&self, operation: RpcOperationDescriptor, input: I, execute: RpcUnaryExecutor<I, O, E>) -> RpcUnaryCallBuilder<I, O, E> { RpcUnaryCallBuilder::new(operation, input, execute) }
  #[must_use] pub fn server_stream<I, O, E>(&self, operation: RpcOperationDescriptor, input: I, open: RpcStreamExecutor<I, O, E>) -> RpcServerStreamCallBuilder<I, O, E> { RpcServerStreamCallBuilder::new(operation, input, open) }
}
