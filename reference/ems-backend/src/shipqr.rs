//! SHIP QR content parsing (normative decoder lives in the EMS backend).
//!
//! Real-world QR content (see synthetic test content below):
//! `SHIP;SKI:<hex40>;ID:<local ship id>;BRAND:<brand>;TYPE:EnergyManagementSystem;
//!  MODEL:<model>;SERIAL:<serial>;CAT:<cat>;FPH256:<hex64>;SPSEC:<hex16>;ENDSHIP;`
//!
//! The QR carries ONLY the LOCAL HEMS identity (SKI, SHIP-ID, FPH) — never Steuerbox
//! data. The Steuerbox anchor is box_evidence from the LAN (mDNS stb_ski).

use anyhow::{Context, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShipQr {
    pub ski: String,
    pub id: String, // local SHIP-ID, e.g. ExampleEMS-HEMS-SNTEST_HEMS-0001
    pub brand: Option<String>,
    pub type_: Option<String>, // EnergyManagementSystem
    pub model: Option<String>,
    pub serial: String,
    pub cat: Option<String>,
    pub fph256: String, // FNN credential fingerprint (SHA-256, uppercase hex)
    /// SHIP security code (SPSEC). Transported in the provisioning payload's
    /// `qr_info` (protected by mTLS) so the MSB can parameterize the box via its
    /// admin channel — but NEVER logged (provisioning bodies are never logged).
    pub spsec: Option<String>,
}

impl ShipQr {
    /// Parse a `SHIP;…;ENDSHIP;` content string.
    pub fn parse(content: &str) -> Result<Self> {
        let body = content
            .trim()
            .strip_prefix("SHIP;")
            .context("QR content must start with SHIP;")?;
        let body = body
            .strip_suffix("ENDSHIP;")
            .or_else(|| body.strip_suffix("ENDSHIP"))
            .context("QR content does not end with ENDSHIP;")?;

        let mut qr = ShipQr {
            spsec: None,
            ..Default::default()
        };
        for field in body.split(';') {
            if field.trim().is_empty() {
                continue;
            }
            let (k, v) = field
                .split_once(':')
                .context("malformed QR field: missing separator")?;
            match k.trim().to_ascii_uppercase().as_str() {
                "SKI" => qr.ski = v.trim().to_string(),
                "ID" => qr.id = v.trim().to_string(),
                "BRAND" => qr.brand = Some(v.trim().to_string()),
                "TYPE" => qr.type_ = Some(v.trim().to_string()),
                "MODEL" => qr.model = Some(v.trim().to_string()),
                "SERIAL" => qr.serial = v.trim().to_string(),
                "CAT" => qr.cat = Some(v.trim().to_string()),
                "FPH256" => qr.fph256 = v.trim().to_uppercase(),
                "SPSEC" => qr.spsec = Some(v.trim().to_string()),
                _ => {} // forward compatibility: unknown fields tolerated
            }
        }
        anyhow::ensure!(!qr.ski.is_empty(), "QR content lacks SKI");
        anyhow::ensure!(!qr.id.is_empty(), "QR content lacks ID");
        Ok(qr)
    }

    /// Canonical SHIP-Info-Datensatz for the provisioning payload (ADR-0017): the
    /// FULL QR content INCLUDING the SPSEC security code — mTLS is its transport;
    /// the MSB forwards it only via the box admin channel and never logs it.
    pub fn format_canonical(&self) -> String {
        let mut out = format!("SHIP;SKI:{};ID:{};", self.ski, self.id);
        if let Some(brand) = &self.brand {
            out.push_str(&format!("BRAND:{brand};"));
        }
        if let Some(t) = &self.type_ {
            out.push_str(&format!("TYPE:{t};"));
        }
        if let Some(model) = &self.model {
            out.push_str(&format!("MODEL:{model};"));
        }
        out.push_str(&format!("SERIAL:{};", self.serial));
        if let Some(cat) = &self.cat {
            out.push_str(&format!("CAT:{cat};"));
        }
        out.push_str(&format!("FPH256:{};", self.fph256));
        if let Some(spsec) = &self.spsec {
            out.push_str(&format!("SPSEC:{spsec};"));
        }
        out.push_str("ENDSHIP;");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONTENT: &str = "SHIP;SKI:0000000000000000000000000000000000000001;ID:ExampleEMS-HEMS-SNTEST_HEMS-0001;BRAND:ExampleEMS;TYPE:EnergyManagementSystem;MODEL:Reference-HEMS;SERIAL:SNTEST_HEMS-0001;CAT:2;FPH256:0000000000000000000000000000000000000000000000000000000000000001;SPSEC:00000000000000000000000000000001;ENDSHIP;";

    #[test]
    fn parses_synthetic_qr_content() {
        let qr = ShipQr::parse(TEST_CONTENT).unwrap();
        assert_eq!(qr.ski, "0000000000000000000000000000000000000001");
        assert_eq!(qr.id, "ExampleEMS-HEMS-SNTEST_HEMS-0001");
        assert_eq!(qr.brand.as_deref(), Some("ExampleEMS"));
        assert_eq!(qr.type_.as_deref(), Some("EnergyManagementSystem"));
        assert_eq!(qr.model.as_deref(), Some("Reference-HEMS"));
        assert_eq!(qr.serial, "SNTEST_HEMS-0001");
        assert_eq!(qr.cat.as_deref(), Some("2"));
        assert_eq!(
            qr.fph256,
            "0000000000000000000000000000000000000000000000000000000000000001"
        );
        assert!(qr.spsec.is_some());
    }

    #[test]
    fn rejects_malformed_content() {
        assert!(ShipQr::parse("NOTSHIP;SKI:x;ID:y;").is_err());
        assert!(ShipQr::parse("SHIP;SKI:x;ID:y;").is_err()); // no ENDSHIP
        assert!(ShipQr::parse("SHIP;SKI:x;ENDSHIP;").is_err()); // no ID
        assert!(ShipQr::parse("SHIP;ID:y;ENDSHIP;").is_err()); // no SKI
    }

    #[test]
    fn malformed_field_error_does_not_echo_qr_content() {
        let err = ShipQr::parse("SHIP;SKI:test;ID:test;SENSITIVE_CANARY;ENDSHIP;")
            .unwrap_err()
            .to_string();
        assert!(!err.contains("SENSITIVE_CANARY"));
    }

    #[test]
    fn unknown_fields_tolerated() {
        let qr = ShipQr::parse("SHIP;SKI:aa;ID:bb;FUTUREFIELD:zz;ENDSHIP;").unwrap();
        assert_eq!(qr.ski, "aa");
        assert_eq!(qr.id, "bb");
    }
}

#[cfg(test)]
mod qr_format_tests {
    use super::*;

    const TEST_CONTENT: &str = "SHIP;SKI:0000000000000000000000000000000000000001;ID:ExampleEMS-HEMS-SNTEST_HEMS-0001;BRAND:ExampleEMS;TYPE:EnergyManagementSystem;MODEL:Reference-HEMS;SERIAL:SNTEST_HEMS-0001;CAT:2;FPH256:0000000000000000000000000000000000000000000000000000000000000001;SPSEC:00000000000000000000000000000001;ENDSHIP;";

    #[test]
    fn canonical_format_carries_full_qr_content() {
        let qr = ShipQr::parse(TEST_CONTENT).unwrap();
        let out = qr.format_canonical();
        assert!(out.starts_with("SHIP;") && out.ends_with("ENDSHIP;"));
        assert!(
            out.contains("FPH256:0000000000000000000000000000000000000000000000000000000000000001")
        );
        // the SPSEC security code rides in the payload (mTLS-protected transport,
        // ADR-0017) — but must never be LOGGED (ems-backend logs metadata only)
        assert_eq!(
            out,
            "SHIP;SKI:0000000000000000000000000000000000000001;ID:ExampleEMS-HEMS-SNTEST_HEMS-0001;BRAND:ExampleEMS;TYPE:EnergyManagementSystem;MODEL:Reference-HEMS;SERIAL:SNTEST_HEMS-0001;CAT:2;FPH256:0000000000000000000000000000000000000000000000000000000000000001;SPSEC:00000000000000000000000000000001;ENDSHIP;"
        );
    }

    #[test]
    fn spsec_without_value_is_omitted() {
        let qr = ShipQr {
            spsec: None,
            ..Default::default()
        };
        assert!(!qr.format_canonical().contains("SPSEC"));
    }
}
