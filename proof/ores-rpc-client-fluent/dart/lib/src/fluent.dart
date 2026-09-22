import 'dart:async';

enum RpcPayloadCodecName { json, messagepack, protobuf }
enum RpcGeneratedStreamMode { unary, serverStream }

final class RpcOperationDescriptor {
  const RpcOperationDescriptor({required this.operationKey, required this.endpoint, required this.streamMode, required this.allowedCodecs, required this.defaultCodec, required this.contractSha256});
  final String operationKey;
  final String endpoint;
  final RpcGeneratedStreamMode streamMode;
  final List<RpcPayloadCodecName> allowedCodecs;
  final RpcPayloadCodecName defaultCodec;
  final String contractSha256;
}

final class RpcCallOptions {
  const RpcCallOptions({this.headers = const {}, this.timeoutMs, this.codec});
  final Map<String, String> headers;
  final int? timeoutMs;
  final RpcPayloadCodecName? codec;
  RpcCallOptions copyWith({Map<String, String>? headers, int? timeoutMs, bool clearTimeout = false, RpcPayloadCodecName? codec, bool clearCodec = false}) => RpcCallOptions(headers: headers ?? this.headers, timeoutMs: clearTimeout ? null : timeoutMs ?? this.timeoutMs, codec: clearCodec ? null : codec ?? this.codec);
}

abstract interface class RpcStreamHandle<T> { Stream<T> get values; FutureOr<void> cancel(); }
typedef RpcUnaryExecutor<TInput, TOutput> = FutureOr<TOutput> Function(RpcOperationDescriptor operation, TInput input, RpcCallOptions options);
typedef RpcStreamExecutor<TInput, TOutput> = FutureOr<RpcStreamHandle<TOutput>> Function(RpcOperationDescriptor operation, TInput input, RpcCallOptions options);

String _headerName(String name) { final value = name.trim().toLowerCase(); if (value.isEmpty || !RegExp(r"^[!#$%&'*+.^_`|~0-9a-z-]+$").hasMatch(value)) { throw ArgumentError.value(name, 'name', 'invalid RPC header name'); } return value; }
int _timeout(int milliseconds) { if (milliseconds < 0 || milliseconds > 0xffffffff) { throw ArgumentError.value(milliseconds, 'milliseconds', 'must fit uint32'); } return milliseconds; }
Map<String, String> _addHeader(Map<String, String> current, String name, String value) => Map.unmodifiable({...current, _headerName(name): value});
RpcPayloadCodecName _codec(RpcOperationDescriptor operation, RpcPayloadCodecName codec) { if (!operation.allowedCodecs.contains(codec)) { throw StateError('RPC ${operation.operationKey} does not admit codec ${codec.name}'); } return codec; }

final class RpcUnaryCallBuilder<TInput, TOutput> {
  RpcUnaryCallBuilder(this.operation, this._input, this._execute, [this.options = const RpcCallOptions()]) { if (operation.streamMode != RpcGeneratedStreamMode.unary) { throw StateError('RPC ${operation.operationKey} is not unary'); } }
  final RpcOperationDescriptor operation; final TInput _input; final RpcUnaryExecutor<TInput, TOutput> _execute; final RpcCallOptions options;
  RpcUnaryCallBuilder<TInput, TOutput> header(String name, String value) => RpcUnaryCallBuilder(operation, _input, _execute, options.copyWith(headers: _addHeader(options.headers, name, value)));
  RpcUnaryCallBuilder<TInput, TOutput> headers(Map<String, String> values) { var merged = options.headers; for (final entry in values.entries) { merged = _addHeader(merged, entry.key, entry.value); } return RpcUnaryCallBuilder(operation, _input, _execute, options.copyWith(headers: merged)); }
  RpcUnaryCallBuilder<TInput, TOutput> timeout(int milliseconds) => RpcUnaryCallBuilder(operation, _input, _execute, options.copyWith(timeoutMs: _timeout(milliseconds)));
  RpcUnaryCallBuilder<TInput, TOutput> codec(RpcPayloadCodecName codec) => RpcUnaryCallBuilder(operation, _input, _execute, options.copyWith(codec: _codec(operation, codec)));
  Future<TOutput> makeCall() => Future<TOutput>.sync(() => _execute(operation, _input, options));
}

final class RpcServerStreamCallBuilder<TInput, TOutput> {
  RpcServerStreamCallBuilder(this.operation, this._input, this._open, [this.options = const RpcCallOptions()]) { if (operation.streamMode != RpcGeneratedStreamMode.serverStream) { throw StateError('RPC ${operation.operationKey} is not server_stream'); } }
  final RpcOperationDescriptor operation; final TInput _input; final RpcStreamExecutor<TInput, TOutput> _open; final RpcCallOptions options;
  RpcServerStreamCallBuilder<TInput, TOutput> header(String name, String value) => RpcServerStreamCallBuilder(operation, _input, _open, options.copyWith(headers: _addHeader(options.headers, name, value)));
  RpcServerStreamCallBuilder<TInput, TOutput> headers(Map<String, String> values) { var merged = options.headers; for (final entry in values.entries) { merged = _addHeader(merged, entry.key, entry.value); } return RpcServerStreamCallBuilder(operation, _input, _open, options.copyWith(headers: merged)); }
  RpcServerStreamCallBuilder<TInput, TOutput> timeout(int milliseconds) => RpcServerStreamCallBuilder(operation, _input, _open, options.copyWith(timeoutMs: _timeout(milliseconds)));
  RpcServerStreamCallBuilder<TInput, TOutput> codec(RpcPayloadCodecName codec) => RpcServerStreamCallBuilder(operation, _input, _open, options.copyWith(codec: _codec(operation, codec)));
  Future<RpcStreamHandle<TOutput>> doStream() => Future<RpcStreamHandle<TOutput>>.sync(() => _open(operation, _input, options));
}

abstract class OresRpcClientBase {
  const OresRpcClientBase();
  RpcUnaryCallBuilder<TInput, TOutput> unary<TInput, TOutput>(RpcOperationDescriptor operation, TInput input, RpcUnaryExecutor<TInput, TOutput> execute) => RpcUnaryCallBuilder(operation, input, execute);
  RpcServerStreamCallBuilder<TInput, TOutput> serverStream<TInput, TOutput>(RpcOperationDescriptor operation, TInput input, RpcStreamExecutor<TInput, TOutput> open) => RpcServerStreamCallBuilder(operation, input, open);
}
