#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

grep -q 'make_call: "make_call"' "$root/contract/main.tsp"
grep -q 'do_stream: "do_stream"' "$root/contract/main.tsp"
grep -q '"enum": \["make_call", "do_stream"\]' "$root/contract/authored.schema.json"
grep -q '"unary_terminal": "make_call"' "$root/runtime-binding.json"
grep -q '"stream_terminal": "do_stream"' "$root/runtime-binding.json"
grep -q '"head_sha": "fe4b3332daee21f1bb7c75a434d7a12a5eb2f258"' "$root/source-pins.json"
grep -q '"merge_sha": "a071dbd727418b0483c3d514c862c0bb4f8633c0"' "$root/source-pins.json"

grep -q 'doStream()' "$root/ts/src/fluent.ts"
! grep -Eq '^[[:space:]]*stream\(\):' "$root/ts/src/fluent.ts"
grep -q 'doStream()' "$root/dart/lib/src/fluent.dart"
! grep -Eq '^[[:space:]]*Future<RpcStreamHandle<TOutput>> stream\(' "$root/dart/lib/src/fluent.dart"
grep -q 'pub fn do_stream(self)' "$root/rust/src/fluent.rs"
! grep -Eq '^[[:space:]]*pub fn stream\(self\)' "$root/rust/src/fluent.rs"
grep -q '^pub fn do_stream(' "$root/gleam/src/ores_http_clients/fluent.gleam"
! grep -Eq '^pub fn stream\(' "$root/gleam/src/ores_http_clients/fluent.gleam"
