import 'dart:async';
import 'package:ores_http_clients/ores_http_clients.dart';
import 'package:test/test.dart';

const operation = RpcOperationDescriptor(operationKey: 'demo.health.get_version', endpoint: '/v1/rpc', streamMode: RpcGeneratedStreamMode.unary, allowedCodecs: [RpcPayloadCodecName.json, RpcPayloadCodecName.messagepack], defaultCodec: RpcPayloadCodecName.json, contractSha256: '0000000000000000000000000000000000000000000000000000000000000000');
const streamOperation = RpcOperationDescriptor(operationKey: 'demo.events.watch', endpoint: '/v1/rpc', streamMode: RpcGeneratedStreamMode.serverStream, allowedCodecs: [RpcPayloadCodecName.json], defaultCodec: RpcPayloadCodecName.json, contractSha256: '0000000000000000000000000000000000000000000000000000000000000000');

final class DemoClient extends OresRpcClientBase { DemoClient(this.execute); final RpcUnaryExecutor<String, String> execute; RpcUnaryCallBuilder<String, String> getVersion(String input) => unary(operation, input, execute); }
final class DemoStreamHandle implements RpcStreamHandle<int> { DemoStreamHandle(this.values); @override final Stream<int> values; @override void cancel() {} }
final class DemoStreamClient extends OresRpcClientBase { DemoStreamClient(this.open); final RpcStreamExecutor<String, int> open; RpcServerStreamCallBuilder<String, int> watch(String input) => serverStream(streamOperation, input, open); }

void main() {
  test('unary chain is inert until makeCall', () async {
    var calls = 0; RpcCallOptions? seen;
    final client = DemoClient((operation, input, options) async { calls += 1; seen = options; return 'version:$input'; });
    final call = client.getVersion('v1').header('X-ORES-TRACE-ID', 'abc').timeout(2500).codec(RpcPayloadCodecName.messagepack);
    expect(calls, 0); expect(await call.makeCall(), 'version:v1'); expect(calls, 1); expect(seen?.headers['x-ores-trace-id'], 'abc'); expect(seen?.timeoutMs, 2500); expect(seen?.codec, RpcPayloadCodecName.messagepack);
  });
  test('server stream is inert until doStream', () async {
    var opens = 0; RpcCallOptions? seen;
    final client = DemoStreamClient((operation, input, options) { opens += 1; seen = options; return DemoStreamHandle(Stream<int>.fromIterable([1, 2])); });
    final call = client.watch('events').header('X-ORES-TRACE-ID', 'abc');
    expect(opens, 0); final handle = await call.doStream(); expect(opens, 1); expect(seen?.headers['x-ores-trace-id'], 'abc'); expect(await handle.values.toList(), [1, 2]);
  });
}
