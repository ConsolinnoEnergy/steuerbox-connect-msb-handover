//! mDNS TXT record parsing + Regel-S derivation from real-world `_ship._tcp` records.
//!
//! Real-world shape (fixtures/golden/mdns_gcph.json):
//!   ["type=GCPH" "ski=<40-hex>" "serial" "register=false" "path=/ship/" "model=…"
//!    "id=i:51593_u:SNTESTGCPH0001" "cat=1" "brand=…" "txtvers=1"]
//!
//! CRITICAL: the `serial` key carries NO value — the serial MUST be derived from the
//! `id=` VPID (`i:<IANA-PEN>_u:<serial>`). Parser correctness is guaranteed by golden
//! fixtures, not by network e2e (AGENTS.md §5).

use serde::{Deserialize, Serialize};

/// Parsed `_ship._tcp` TXT record (tolerant: missing keys are None).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TxtRecord {
    pub txtvers: Option<String>,
    /// recognized Steuerbox type: ControlBox | GCPH
    pub stb_type: Option<String>,
    /// valueless `serial` key or serial=… — real records have NO value here
    pub serial_hint: Option<String>,
    /// full VPID: `i:<PEN>_u:<VPID>`
    pub id: Option<String>,
    pub ski: Option<String>,
    pub model: Option<String>,
    pub brand: Option<String>,
    pub path: Option<String>,
    pub register: Option<bool>,
}

impl TxtRecord {
    /// Parse a TXT key/value list. Keys without `=` are legal and carry an empty value.
    pub fn parse(entries: &[&str]) -> Self {
        let mut rec = TxtRecord::default();
        for entry in entries {
            let (k, v) = match entry.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim().to_string()),
                None => (entry.trim(), String::new()), // valueless key
            };
            match k {
                "txtvers" => rec.txtvers = Some(v),
                "type" => rec.stb_type = Some(v),
                "serial" => rec.serial_hint = Some(v), // often empty — see doc comment
                "id" => rec.id = Some(v),
                "ski" => rec.ski = Some(v),
                "model" => rec.model = Some(v),
                "brand" => rec.brand = Some(v),
                "path" => rec.path = Some(v),
                "register" => rec.register = Some(v == "true"),
                _ => {} // unknown keys are tolerated (forward compatibility)
            }
        }
        rec
    }

    /// Steuerbox serial extracted from the VPID (part after `_u:`).
    pub fn vpid_serial(&self) -> Option<&str> {
        let id = self.id.as_deref()?;
        let idx = id.find("_u:")?;
        Some(&id[idx + 3..])
    }

    /// Regel-S derivation: split at the first `-`; no hyphen → whole string.
    /// Falls back to a valued `serial=` key only when `id=` is absent.
    pub fn regel_s_serial(&self) -> Option<&str> {
        let candidate = match (self.vpid_serial(), self.serial_hint.as_deref()) {
            (Some(v), _) => v,
            (None, Some(s)) if !s.is_empty() => s,
            _ => return None,
        };
        Some(steuerbox_protocol::regel_s_base(candidate))
    }

    /// box_evidence ship_id: the full VPID (real-world: hems_eebus_stb_ship_id).
    pub fn box_ship_id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn golden_gcph_record() {
        let fixture: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/golden/mdns_gcph.json"
            )))
            .expect("golden fixture"),
        )
        .unwrap();
        let txt: Vec<String> = fixture["txt"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let refs: Vec<&str> = txt.iter().map(|s| s.as_str()).collect();

        let rec = TxtRecord::parse(&refs);
        assert_eq!(rec.stb_type.as_deref(), Some("GCPH"));
        // `serial` key has NO value in the real record:
        assert_eq!(rec.serial_hint.as_deref(), Some(""));
        // serial must come from id= VPID:
        assert_eq!(rec.vpid_serial(), Some("SNTESTGCPH0001"));
        assert_eq!(rec.regel_s_serial(), Some("SNTESTGCPH0001"));
        assert_eq!(rec.box_ship_id(), Some("i:51593_u:SNTESTGCPH0001"));
        assert_eq!(rec.ski.as_deref(), Some("SNTESTSKIGCPH0001"));

        // expected block from the fixture must match the parser output
        let expected = &fixture["expected"];
        assert_eq!(
            rec.regel_s_serial().unwrap(),
            expected["stb_serial"].as_str().unwrap()
        );
        assert_eq!(
            rec.box_ship_id().unwrap(),
            expected["stb_vpid"].as_str().unwrap()
        );
    }

    #[test]
    fn suffix_form_derives_base() {
        let rec = TxtRecord::parse(&["id=i:51593_u:SNTEST0002-A1", "ski=SKISKI"]);
        assert_eq!(rec.regel_s_serial(), Some("SNTEST0002"));
        assert_eq!(rec.vpid_serial(), Some("SNTEST0002-A1"));
    }

    #[test]
    fn valued_serial_fallback_without_id() {
        let rec = TxtRecord::parse(&["serial=SNTEST0003"]);
        assert_eq!(rec.regel_s_serial(), Some("SNTEST0003"));
    }

    #[test]
    fn unknown_keys_tolerated() {
        let rec = TxtRecord::parse(&["id=i:1_u:SNTEST0004", "future-key=whatever"]);
        assert_eq!(rec.regel_s_serial(), Some("SNTEST0004"));
    }

    #[test]
    fn empty_record_is_none() {
        assert_eq!(TxtRecord::parse(&[]).regel_s_serial(), None);
    }
}
