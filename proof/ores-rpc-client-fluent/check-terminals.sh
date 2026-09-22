#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

require_fixed() {
  label="$1"
  needle="$2"
  path="$3"
  if ! grep -Fq -- "$needle" "$path"; then
    echo "missing terminal proof: $label ($path)" >&2
    exit 1
  fi
  echo "PASS: $label"
}

forbid_ere() {
  label="$1"
  pattern="$2"
  path="$3"
  if grep -Eq -- "$pattern" "$path"; then
    echo "forbidden terminal alias present: $label ($path)" >&2
    exit 1
  fi
  echo "PASS: $label"
}

require_fixed "TypeSpec unary terminal" 'make_call: "make_call"' "$root/contract/main.tsp"
require_fixed "TypeSpec stream terminal" 'do_stream: "do_stream"' "$root/contract/main.tsp"
require_fixed "JSON Schema terminal enum" '"enum": ["make_call", "do_stream"]' "$root/contract/authored.schema.json"
require_fixed "runtime binding unary terminal" '"unary_terminal": "make_call"' "$root/runtime-binding.json"
require_fixed "runtime binding stream terminal" '"stream_terminal": "do_stream"' "$root/runtime-binding.json"
require_fixed "exact ores-http-clients head" '"head_sha": "fe4b3332daee21f1bb7c75a434d7a12a5eb2f258"' "$root/source-pins.json"
require_fixed "exact ores-interfaces merge" '"merge_sha": "a071dbd727418b0483c3d514c862c0bb4f8633c0"' "$root/source-pins.json"

require_fixed "TypeScript doStream terminal" 'doStream()' "$root/ts/src/fluent.ts"
forbid_ere "TypeScript stream() execution terminal" '^[[:space:]]*stream\(\):' "$root/ts/src/fluent.ts"

require_fixed "Dart doStream terminal" 'doStream()' "$root/dart/lib/src/fluent.dart"
forbid_ere "Dart stream() execution terminal" '^[[:space:]]*Future<RpcStreamHandle<TOutput>> stream\(' "$root/dart/lib/src/fluent.dart"

require_fixed "Rust do_stream terminal" 'pub fn do_stream(self)' "$root/rust/src/fluent.rs"
forbid_ere "Rust stream() execution terminal" '^[[:space:]]*pub fn stream\(self\)' "$root/rust/src/fluent.rs"

require_fixed "Gleam do_stream terminal" 'pub fn do_stream(' "$root/gleam/src/ores_http_clients/fluent.gleam"
forbid_ere "Gleam stream() execution terminal" '^pub fn stream\(' "$root/gleam/src/ores_http_clients/fluent.gleam"
