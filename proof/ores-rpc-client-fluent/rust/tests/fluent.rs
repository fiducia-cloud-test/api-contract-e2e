use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};
use ores_http_clients::{BoxRpcStream, OresRpcClientBase, RpcCallOptions, RpcGeneratedStreamMode, RpcOperationDescriptor, RpcPayloadCodecName, RpcStreamExecutor, RpcUnaryExecutor};

const OPERATION: RpcOperationDescriptor = RpcOperationDescriptor { operation_key: "demo.health.get_version", endpoint: "/v1/rpc", stream_mode: RpcGeneratedStreamMode::Unary, allowed_codecs: &[RpcPayloadCodecName::Json, RpcPayloadCodecName::Messagepack], default_codec: RpcPayloadCodecName::Json, contract_sha256: "0000000000000000000000000000000000000000000000000000000000000000" };
const STREAM_OPERATION: RpcOperationDescriptor = RpcOperationDescriptor { operation_key: "demo.events.watch", endpoint: "/v1/rpc", stream_mode: RpcGeneratedStreamMode::ServerStream, allowed_codecs: &[RpcPayloadCodecName::Json], default_codec: RpcPayloadCodecName::Json, contract_sha256: "0000000000000000000000000000000000000000000000000000000000000000" };

#[tokio::test]
async fn unary_chain_is_inert_until_make_call() {
  let calls = Arc::new(AtomicUsize::new(0)); let seen = Arc::new(Mutex::new(None::<RpcCallOptions>)); let calls_for_executor = Arc::clone(&calls); let seen_for_executor = Arc::clone(&seen);
  let execute: RpcUnaryExecutor<String, String, String> = Arc::new(move |_operation, input, options| { let calls = Arc::clone(&calls_for_executor); let seen = Arc::clone(&seen_for_executor); Box::pin(async move { calls.fetch_add(1, Ordering::SeqCst); *seen.lock().expect("seen lock") = Some(options); Ok(format!("version:{input}")) }) });
  let call = OresRpcClientBase.unary(OPERATION, "v1".to_owned(), execute).header("X-ORES-TRACE-ID", "abc").timeout(2500).codec(RpcPayloadCodecName::Messagepack);
  assert_eq!(calls.load(Ordering::SeqCst), 0); let value = call.make_call().await.expect("unary call"); assert_eq!(value, "version:v1"); assert_eq!(calls.load(Ordering::SeqCst), 1); let options = seen.lock().expect("seen lock").clone().expect("captured options"); assert_eq!(options.headers.get("x-ores-trace-id").map(String::as_str), Some("abc")); assert_eq!(options.timeout_ms, Some(2500)); assert_eq!(options.codec, Some(RpcPayloadCodecName::Messagepack));
}

#[tokio::test]
async fn server_stream_chain_is_inert_until_do_stream() {
  let opens = Arc::new(AtomicUsize::new(0)); let seen = Arc::new(Mutex::new(None::<RpcCallOptions>)); let opens_for_executor = Arc::clone(&opens); let seen_for_executor = Arc::clone(&seen);
  let open: RpcStreamExecutor<String, usize, String> = Arc::new(move |_operation, _input, options| { let opens = Arc::clone(&opens_for_executor); let seen = Arc::clone(&seen_for_executor); Box::pin(async move { opens.fetch_add(1, Ordering::SeqCst); *seen.lock().expect("seen lock") = Some(options); let values: BoxRpcStream<usize, String> = Box::pin(futures_util::stream::iter([Ok(1usize), Ok(2usize)])); Ok(values) }) });
  let call = OresRpcClientBase.server_stream(STREAM_OPERATION, "events".to_owned(), open).header("X-ORES-TRACE-ID", "abc");
  assert_eq!(opens.load(Ordering::SeqCst), 0); let _stream = call.do_stream().await.expect("server stream open"); assert_eq!(opens.load(Ordering::SeqCst), 1); let options = seen.lock().expect("seen lock").clone().expect("captured options"); assert_eq!(options.headers.get("x-ores-trace-id").map(String::as_str), Some("abc"));
}
