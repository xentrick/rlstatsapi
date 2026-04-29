#![no_main]

use libfuzzer_sys::fuzz_target;
use rlstatsapi::EventEnvelope;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = serde_json::from_slice::<Value>(data) {
        let serialized = value.to_string();
        if let Ok(envelope) =
            serde_json::from_str::<EventEnvelope<Value>>(&serialized)
        {
            let _ = envelope.event;
            let _ = envelope.data;
        }
    }
});
