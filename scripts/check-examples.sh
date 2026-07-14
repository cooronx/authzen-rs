#!/bin/sh
set -eu

run_example() {
    name="$1"
    expected="$2"
    output=$(cargo run --quiet --all-features --example "$name" 2>/dev/null)
    case "$output" in
        *"$expected"*) printf 'PASS %s: %s\n' "$name" "$output" ;;
        *) printf 'FAIL %s: expected %s, got %s\n' "$name" "$expected" "$output" >&2; exit 1 ;;
    esac
}

unset AUTHZEN_PDP_URL
run_example client "offline request="
run_example custom_pdp "alice allowed=false"
run_example tower_pep "private status=403"
run_example tower_pdp 'body={"decision":true}'
