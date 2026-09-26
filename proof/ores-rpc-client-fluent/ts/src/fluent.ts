export type RpcPayloadCodecName = "json" | "messagepack" | "protobuf";
export type RpcGeneratedStreamMode = "unary" | "server_stream";

/** Fleet-generic operation descriptor mirrored from ores-interfaces/rpc-client-call/v1. */
export interface RpcOperationDescriptor {
  readonly operationKey: string;
  readonly endpoint: string;
  readonly streamMode: RpcGeneratedStreamMode;
  readonly allowedCodecs: readonly RpcPayloadCodecName[];
  readonly defaultCodec: RpcPayloadCodecName;
  readonly contractSha256: string;
}

/** Runtime-only options accumulated by an immutable fluent call chain. */
export interface RpcCallOptions {
  readonly headers: Readonly<Record<string, string>>;
  readonly timeoutMs?: number;
  readonly codec?: RpcPayloadCodecName;
  readonly signal?: AbortSignal;
}

export interface RpcStreamHandle<T> extends AsyncIterable<T> {
  cancel(): Promise<void> | void;
}

export type RpcUnaryExecutor<TInput, TOutput> = (
  operation: RpcOperationDescriptor,
  input: TInput,
  options: RpcCallOptions,
) => Promise<TOutput> | TOutput;

export type RpcStreamExecutor<TInput, TOutput> = (
  operation: RpcOperationDescriptor,
  input: TInput,
  options: RpcCallOptions,
) => Promise<RpcStreamHandle<TOutput>> | RpcStreamHandle<TOutput>;

function normalizedHeaderName(name: string): string {
  const value = name.trim().toLowerCase();
  if (!value) throw new Error("RPC header name must be non-empty");
  if (!/^[!#$%&'*+.^_`|~0-9a-z-]+$/.test(value)) throw new Error(`invalid RPC header name: ${name}`);
  return value;
}

function normalizedTimeout(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error("RPC timeout must be an unsigned 32-bit millisecond value");
  }
  return value;
}

function withHeader(options: RpcCallOptions, name: string, value: string): RpcCallOptions {
  return Object.freeze({
    ...options,
    headers: Object.freeze({...options.headers, [normalizedHeaderName(name)]: String(value)}),
  });
}

function withHeaders(options: RpcCallOptions, headers: Readonly<Record<string, string>>): RpcCallOptions {
  let next = options;
  for (const [name, value] of Object.entries(headers)) next = withHeader(next, name, value);
  return next;
}

function withCodec(options: RpcCallOptions, operation: RpcOperationDescriptor, codec: RpcPayloadCodecName): RpcCallOptions {
  if (!operation.allowedCodecs.includes(codec)) {
    throw new Error(`RPC ${operation.operationKey} does not admit codec ${codec}`);
  }
  return Object.freeze({...options, codec});
}

function initialOptions(): RpcCallOptions {
  return Object.freeze({headers: Object.freeze({})});
}

export class RpcUnaryCallBuilder<TInput, TOutput> {
  constructor(
    readonly operation: RpcOperationDescriptor,
    private readonly input: TInput,
    private readonly execute: RpcUnaryExecutor<TInput, TOutput>,
    readonly options: RpcCallOptions = initialOptions(),
  ) {
    if (operation.streamMode !== "unary") {
      throw new Error(`RPC ${operation.operationKey} is ${operation.streamMode}; unary builder refused`);
    }
    Object.freeze(this);
  }

  header(name: string, value: string): RpcUnaryCallBuilder<TInput, TOutput> {
    return new RpcUnaryCallBuilder(this.operation, this.input, this.execute, withHeader(this.options, name, value));
  }

  headers(values: Readonly<Record<string, string>>): RpcUnaryCallBuilder<TInput, TOutput> {
    return new RpcUnaryCallBuilder(this.operation, this.input, this.execute, withHeaders(this.options, values));
  }

  timeout(milliseconds: number): RpcUnaryCallBuilder<TInput, TOutput> {
    return new RpcUnaryCallBuilder(this.operation, this.input, this.execute, Object.freeze({...this.options, timeoutMs: normalizedTimeout(milliseconds)}));
  }

  codec(codec: RpcPayloadCodecName): RpcUnaryCallBuilder<TInput, TOutput> {
    return new RpcUnaryCallBuilder(this.operation, this.input, this.execute, withCodec(this.options, this.operation, codec));
  }

  signal(signal: AbortSignal): RpcUnaryCallBuilder<TInput, TOutput> {
    return new RpcUnaryCallBuilder(this.operation, this.input, this.execute, Object.freeze({...this.options, signal}));
  }

  /** Sole unary network terminal. No preceding chain operation performs I/O. */
  makeCall(): Promise<TOutput> {
    return Promise.resolve(this.execute(this.operation, this.input, this.options));
  }
}

export class RpcServerStreamCallBuilder<TInput, TOutput> {
  constructor(
    readonly operation: RpcOperationDescriptor,
    private readonly input: TInput,
    private readonly open: RpcStreamExecutor<TInput, TOutput>,
    readonly options: RpcCallOptions = initialOptions(),
  ) {
    if (operation.streamMode !== "server_stream") {
      throw new Error(`RPC ${operation.operationKey} is ${operation.streamMode}; stream builder refused`);
    }
    Object.freeze(this);
  }

  header(name: string, value: string): RpcServerStreamCallBuilder<TInput, TOutput> {
    return new RpcServerStreamCallBuilder(this.operation, this.input, this.open, withHeader(this.options, name, value));
  }

  headers(values: Readonly<Record<string, string>>): RpcServerStreamCallBuilder<TInput, TOutput> {
    return new RpcServerStreamCallBuilder(this.operation, this.input, this.open, withHeaders(this.options, values));
  }

  timeout(milliseconds: number): RpcServerStreamCallBuilder<TInput, TOutput> {
    return new RpcServerStreamCallBuilder(this.operation, this.input, this.open, Object.freeze({...this.options, timeoutMs: normalizedTimeout(milliseconds)}));
  }

  codec(codec: RpcPayloadCodecName): RpcServerStreamCallBuilder<TInput, TOutput> {
    return new RpcServerStreamCallBuilder(this.operation, this.input, this.open, withCodec(this.options, this.operation, codec));
  }

  signal(signal: AbortSignal): RpcServerStreamCallBuilder<TInput, TOutput> {
    return new RpcServerStreamCallBuilder(this.operation, this.input, this.open, Object.freeze({...this.options, signal}));
  }

  /** Sole server-stream OPEN terminal. No preceding chain operation performs I/O. */
  doStream(): Promise<RpcStreamHandle<TOutput>> {
    return Promise.resolve(this.open(this.operation, this.input, this.options));
  }
}

/**
 * Parent class for the alternate ores-stack generated TypeScript lane.
 * Product-specific generated clients subclass this; transport execution stays
 * injectable so this package never owns product schemas or operation names.
 */
export abstract class OresRpcClientBase {
  protected unary<TInput, TOutput>(
    operation: RpcOperationDescriptor,
    input: TInput,
    execute: RpcUnaryExecutor<TInput, TOutput>,
  ): RpcUnaryCallBuilder<TInput, TOutput> {
    return new RpcUnaryCallBuilder(operation, input, execute);
  }

  protected serverStream<TInput, TOutput>(
    operation: RpcOperationDescriptor,
    input: TInput,
    open: RpcStreamExecutor<TInput, TOutput>,
  ): RpcServerStreamCallBuilder<TInput, TOutput> {
    return new RpcServerStreamCallBuilder(operation, input, open);
  }
}
