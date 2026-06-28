#![no_main]
use libfuzzer_sys::fuzz_target;
use plist::Value;

// Fuzz the REAL attacker-facing plist path. The server parses RTSP request bodies
// with `plist::from_bytes::<plist::Value>` and then walks the result with these
// accessors (see raop::handlers_ap2::handle_setup and friends). Mirror that here so
// any panic in parse-or-traverse on malformed peer input is caught.
fuzz_target!(|data: &[u8]| {
    if let Ok(val) = plist::from_bytes::<Value>(data) {
        walk(&val);
    }
});

fn walk(val: &Value) {
    let _ = val.as_boolean();
    let _ = val.as_unsigned_integer();
    let _ = val.as_signed_integer();
    let _ = val.as_real();
    let _ = val.as_string();
    let _ = val.as_data();
    if let Some(arr) = val.as_array() {
        for v in arr {
            walk(v);
        }
    }
    if let Some(dict) = val.as_dictionary() {
        for (_k, v) in dict {
            walk(v);
        }
    }
}
