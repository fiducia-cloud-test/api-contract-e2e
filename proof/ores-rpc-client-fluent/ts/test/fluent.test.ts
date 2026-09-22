import assert from "node:assert/strict";
import test from "node:test";

import {
  OresRpcClientBase,
  type RpcOperationDescriptor,
  type RpcStreamHandle,
} from "../src/fluent.js";

const digest = "0".repeat(64);
const unaryOperation: RpcOperationDescriptor = { operationKey: "demo.health.get_version", endpoint: "/v1/rpc", streamMode: "unary", allowedCodecs: ["json", "messagepack"], defaultCodec: "json", contractSha256: digest };
const streamOperation: RpcOperationDescriptor = { operationKey: "demo.events.watch", endpoint: "/v1/rpc", streamMode: "server_stream", allowedCodecs: ["json"], defaultCodec: "json", contractSha256: digest };

class DemoClient extends OresRpcClientBase {
  calls = 0;
  opens = 0;
  lastOptions: unknown;
  getVersion(input: {id: string}) { return this.unary(unaryOperation, input, async (_operation, value, options) => { this.calls += 1; this.lastOptions = options; return {version: value.id}; }); }
  watch() { return this.serverStream(streamOperation, {}, async (_operation, _input, options) => { this.opens += 1; this.lastOptions = options; const values = [1, 2]; const handle: RpcStreamHandle<number> = { async cancel() {}, async *[Symbol.asyncIterator]() { for (const value of values) yield value; } }; return handle; }); }
}

test("unary chain performs no I/O before makeCall", async () => {
  const client = new DemoClient();
  const call = client.getVersion({id: "v1"}).header("X-ORES-TRACE-ID", "abc").timeout(2500).codec("messagepack");
  assert.equal(client.calls, 0);
  assert.deepEqual(await call.makeCall(), {version: "v1"});
  assert.equal(client.calls, 1);
  assert.deepEqual(client.lastOptions, {headers: {"x-ores-trace-id": "abc"}, timeoutMs: 2500, codec: "messagepack"});
});

test("server stream opens only at doStream terminal", async () => {
  const client = new DemoClient();
  const call = client.watch().header("X-ORES-TRACE-ID", "abc");
  assert.equal(client.opens, 0);
  const handle = await call.doStream();
  assert.equal(client.opens, 1);
  const values: number[] = [];
  for await (const value of handle) values.push(value);
  assert.deepEqual(values, [1, 2]);
});
