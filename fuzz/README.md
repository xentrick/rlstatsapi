Fuzzing Guide

Prerequisites
- Install cargo-fuzz: cargo install cargo-fuzz
- Install nightly toolchain: rustup toolchain install nightly
- Use a C toolchain compatible with libFuzzer on your platform

Targets
- parse_stats_event: exercises parse_stats_event with arbitrary UTF-8 input
- event_envelope_value: exercises EventEnvelope<Value> deserialization from arbitrary JSON-like inputs

Run fuzzers from repository root
- cargo +nightly fuzz run parse_stats_event
- cargo +nightly fuzz run event_envelope_value

Optional corpus usage
- cargo +nightly fuzz run parse_stats_event fuzz/corpus/parse_stats_event
- cargo +nightly fuzz run event_envelope_value fuzz/corpus/event_envelope_value

Artifacts
- Crashes and minimized inputs are stored under fuzz/artifacts/
- Seed corpora can be stored under fuzz/corpus/
