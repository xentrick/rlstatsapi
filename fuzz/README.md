Fuzzing Guide

Prerequisites
- Install cargo-fuzz: cargo install cargo-fuzz
- Use a C toolchain compatible with libFuzzer on your platform

Targets
- parse_stats_event: exercises parse_stats_event with arbitrary UTF-8 input
- event_envelope_value: exercises EventEnvelope<Value> deserialization from arbitrary JSON-like inputs

Run fuzzers from repository root
- cargo fuzz run parse_stats_event
- cargo fuzz run event_envelope_value

Optional corpus usage
- cargo fuzz run parse_stats_event fuzz/corpus/parse_stats_event
- cargo fuzz run event_envelope_value fuzz/corpus/event_envelope_value

Artifacts
- Crashes and minimized inputs are stored under fuzz/artifacts/
- Seed corpora can be stored under fuzz/corpus/
