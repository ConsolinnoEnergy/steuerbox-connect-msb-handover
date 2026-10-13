//! E2E scenario runner — LOCAL, without deployment.
//!
//! Spawns the reference services (wellknown, msb, box-mock, hems-mock) as local processes
//! with real certificates from `test/ca` and runs the normative scenarios (AGENTS.md §7):
//! HappyPath_ZeroTouch, SuffixVPID, TxtSerial, TriState_Unknown, BoxEvidenceMismatch,
//! RogueClientCA, KillSwitch, ClientCertRotation, ClientCARotation, RateLimit,
//! IdempotentPost, AntiEnumeration, ErrorContract, FleetMismatch (ADR-0018),
//! NetworkBoundaries.
//!
//! Cloud-side only: the reference HEMS has already received everything from the
//! Steuerbox via mDNS and forwarded it (`hems_eebus_*`). The e2e suite feeds that
//! observation into hems-mock as env input (`HEMS_TXT_RECORD`) through the SAME
//! normative parser — no mDNS is exercised here.
//!
//! Run: `make e2e` (builds, regenerates CA, runs suite). Single scenario: `E2E_SCENARIO=Name`.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{ensure, Result};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

pub const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
pub const CA: &str = "test/ca/out";

const MSB_API: &str = "127.0.0.1:18443";
pub const MSB_API_URL: &str = "https://127.0.0.1:18443";
const MSB_RATE_URL: &str = "https://127.0.0.1:18453";
const MSB_ADMIN: &str = "127.0.0.1:18444";
const WK_URL: &str = "https://127.0.0.1:19443";
const WK_BIND: &str = "127.0.0.1:19443";
const BOX_ADMIN: &str = "http://127.0.0.1:18642";
const BOX_SHIP: &str = "127.0.0.1:19543";
const HEMS_URL: &str = "http://127.0.0.1:18643";
const HEMS_BIND: &str = "127.0.0.1:18643";
const COMPOSE_FILE: &str = "deploy/compose/docker-compose.yml";

const MSB_TOKEN: &str = "admin-test-token";
const BOX_TOKEN: &str = "box-admin-token";
const GOLDEN_TXT: &str = r#"["type=GCPH","ski=SNTESTSKIGCPH0001","serial","register=false","path=/ship/","model=Control Module","id=i:51593_u:SNTEST0001","cat=1","brand=Test Smart Energy GmbH","txtvers=1"]"#;

pub const SN_GOLDEN: &str = "SNTEST0001";
const SN_SECOND: &str = "SNTEST0002";

/// HEMS ship_id of the example EMS device (SHIP-ID shape `i:<IANA-PEN>_u:<VPID>`,
/// ADR-0018: the PEN must match the well-known iana_pen).
pub const HEMS_SHIP_ID_LOCAL: &str = "i:12345_u:ExampleEMS-HEMS-SNTEST_HEMS-0001";

/// Synthetic IANA PEN of the example EMS manufacturer. The well-known
/// server publishes it; the MSB rejects provisioning whose ship_id carries a
/// different PEN (ADR-0018 fleet binding).
const PEN_EMS: &str = "12345";

// --- stack management --------------------------------------------------------

/// Service endpoints as seen by the test harness (host side in both modes).
#[derive(Clone)]
pub struct Endpoints {
    pub msb_url: String,
    msb_rate_url: String,
    box_admin: String,
    box_ship: String,
    hems_url: String,
}

pub struct Stack {
    children: Vec<Child>,
    pub log_dir: std::path::PathBuf,
    pins_dir: String,
    pub endpoints: Endpoints,
    /// Some(project-dir) = docker mode: teardown via `docker compose down` on drop
    compose: Option<std::path::PathBuf>,
}

impl Drop for Stack {
    fn drop(&mut self) {
        for c in &mut self.children {
            if let Some(pid) = c.id() {
                let _ = std::process::Command::new("kill")
                    .args(["-INT", &pid.to_string()])
                    .status();
                let deadline = std::time::Instant::now() + Duration::from_secs(2);
                while c.try_wait().ok().flatten().is_none() && std::time::Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                let _ = c.start_kill();
            }
        }
        if let Some(dir) = &self.compose {
            let _ = std::process::Command::new("docker")
                .args([
                    "compose",
                    "-f",
                    "deploy/compose/docker-compose.yml",
                    "down",
                    "-t",
                    "2",
                ])
                .current_dir(dir)
                .output();
        }
    }
}

pub async fn wait_port(port: &str, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if TcpStream::connect(port).await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!("service on {port} did not come up");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn log_file(dir: &std::path::Path, name: &str) -> std::fs::File {
    std::fs::File::create(dir.join(format!("{name}.log"))).expect("log file")
}

fn binary_path(name: &str) -> std::path::PathBuf {
    std::env::var_os("E2E_BIN_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(ROOT).join("reference/target/debug"))
        .join(name)
}

fn inventory() -> serde_json::Value {
    serde_json::json!({
        SN_GOLDEN: {
            "ship_id": format!("i:51593_u:{SN_GOLDEN}"),
            "ski": "SNTESTSKIGCPH0001",
        },
        SN_SECOND: {
            "ship_id": format!("i:51593_u:{SN_SECOND}"),
            "ski": "SNTESTSKIGCPH0002",
        }
    })
}

/// Spawn the full stack (msb + wellknown + box-mock + hems-mock) as local processes.
async fn start_stack_local(log_dir: &std::path::Path, opts: StackOpts) -> Result<Stack> {
    // Every stack represents a new bilateral qualification with fresh test PKI.
    let _ = std::fs::remove_file(log_dir.join("client-ca.pin"));
    for address in [
        MSB_API,
        MSB_ADMIN,
        "127.0.0.1:18453",
        "127.0.0.1:18454",
        WK_BIND,
        "127.0.0.1:18642",
        BOX_SHIP,
        HEMS_BIND,
    ] {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|e| anyhow::anyhow!("test port {address} is already in use: {e}"))?;
        drop(listener);
    }
    let mut children = Vec::new();
    let pins_dir = opts
        .pins_dir
        .clone()
        .unwrap_or_else(|| format!("{CA}/pins"));

    let spawn = |name: &'static str, bin: &str, env: Vec<(&str, String)>| {
        let log = log_file(log_dir, name);
        let mut cmd = Command::new(binary_path(bin));
        cmd.current_dir(ROOT)
            .env("RUST_LOG", "info")
            .stderr(Stdio::from(log.try_clone()?))
            .stdout(Stdio::from(log));
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.kill_on_drop(true).spawn()
    };

    // wellknown
    let wk = spawn(
        "wellknown",
        "wellknown",
        vec![
            ("WK_BIND", WK_BIND.to_string()),
            ("WK_TLS_CERT", format!("{CA}/wellknown-chain.pem")),
            ("WK_TLS_KEY", format!("{CA}/wellknown-key.pem")),
            ("WK_CLIENT_CA_PEM", format!("{CA}/client-ca-cert.pem")),
            ("WK_IANA_PEN", PEN_EMS.into()),
        ],
    )?;
    children.push(wk);

    // msb
    let msb = spawn(
        "msb",
        "msb",
        vec![
            ("MSB_BIND", MSB_API.to_string()),
            ("MSB_ADMIN_BIND", MSB_ADMIN.to_string()),
            ("MSB_ADMIN_TOKEN", MSB_TOKEN.to_string()),
            ("MSB_TLS_CERT", format!("{CA}/msb-chain.pem")),
            ("MSB_TLS_KEY", format!("{CA}/msb-key.pem")),
            ("MSB_CLIENT_CA_DIR", pins_dir.clone()),
            ("MSB_MSB_ID", "msb-reference".into()),
            ("MSB_INVENTORY", serde_json::to_string(&inventory())?),
            ("MSB_BOX_ADMIN_URL", BOX_ADMIN.to_string()),
            ("MSB_BOX_ADMIN_TOKEN", BOX_TOKEN.into()),
            (
                "MSB_RATE_LIMIT",
                opts.rate_limit.unwrap_or(10_000).to_string(),
            ),
            ("MSB_IANA_PEN", PEN_EMS.into()),
        ],
    )?;
    children.push(msb);

    // msb-ratelimit: dedicated low-quota instance for the RateLimit scenario
    // (mirrors the msb-ratelimit service in deploy/compose)
    let msb_rate = spawn(
        "msb-ratelimit",
        "msb",
        vec![
            ("MSB_BIND", "127.0.0.1:18453".to_string()),
            ("MSB_ADMIN_BIND", "127.0.0.1:18454".to_string()),
            ("MSB_ADMIN_TOKEN", MSB_TOKEN.to_string()),
            ("MSB_TLS_CERT", format!("{CA}/msb-chain.pem")),
            ("MSB_TLS_KEY", format!("{CA}/msb-key.pem")),
            ("MSB_CLIENT_CA_DIR", pins_dir.clone()),
            ("MSB_MSB_ID", "msb-reference".into()),
            ("MSB_INVENTORY", serde_json::to_string(&inventory())?),
            ("MSB_IANA_PEN", PEN_EMS.into()),
            ("MSB_RATE_LIMIT", "3".to_string()),
        ],
    )?;
    children.push(msb_rate);

    // box-mock
    let box_mock = spawn(
        "box-mock",
        "box-mock",
        vec![
            ("BOX_SERIAL", SN_GOLDEN.into()),
            ("BOX_TYPE", "GCPH".into()),
            ("BOX_PEN", "51593".into()),
            ("BOX_SKI", "SNTESTSKIGCPH0001".into()),
            ("BOX_SHIP_PORT", "19543".into()),
            ("BOX_ADMIN_PORT", "18642".into()),
            ("BOX_ADMIN_TOKEN", BOX_TOKEN.into()),
        ],
    )?;
    children.push(box_mock);

    // hems-mock: cloud-side stub — the Steuerbox observation arrives as env input
    // (what the reference HEMS reported), parsed through the normative TXT parser.
    let hems_env: Vec<(&str, String)> = vec![
        ("HEMS_BIND", HEMS_BIND.to_string()),
        ("HEMS_SERIAL", "SNTEST_HEMS-0001".into()),
        ("HEMS_LOCAL_SKI", "SNTESTLOCASKI0001".into()),
        ("HEMS_TXT_RECORD", GOLDEN_TXT.into()),
        ("EMS_QR_PUB_PEM", format!("{CA}/ems-qr-pub.pem")),
    ];
    let hems = spawn("hems-mock", "hems-mock", hems_env)?;
    children.push(hems);

    for port in [
        MSB_API,
        "127.0.0.1:18453",
        WK_BIND,
        "127.0.0.1:18642",
        HEMS_BIND,
    ] {
        wait_port(port, Duration::from_secs(15)).await?;
    }

    Ok(Stack {
        children,
        log_dir: log_dir.to_path_buf(),
        pins_dir,
        endpoints: Endpoints {
            msb_url: MSB_API_URL.into(),
            msb_rate_url: MSB_RATE_URL.into(),
            box_admin: BOX_ADMIN.into(),
            box_ship: BOX_SHIP.into(),
            hems_url: HEMS_URL.into(),
        },
        compose: None,
    })
}

/// Start the shared simulated stack for a consumer-specific acceptance test.
pub async fn start_local_stack(log_dir: &std::path::Path) -> Result<Stack> {
    start_stack_local(log_dir, StackOpts::default()).await
}

/// Start the full stack as docker services (deploy/compose, backend-to-backend).
/// MSB state resets per scenario via `docker compose restart`.
async fn start_stack_docker(root: &std::path::Path, wait: &[&str]) -> Result<Stack> {
    let _ = std::fs::remove_file(root.join(CA).join("backend-pins/client-ca.pin"));
    run_docker(root, &["up", "-d", "--build"])?;
    for port in wait {
        wait_port(port, Duration::from_secs(120)).await?;
    }
    let log_dir = root.join("reference/target/e2e-logs/docker");
    std::fs::create_dir_all(&log_dir)?;
    Ok(Stack {
        children: vec![],
        log_dir,
        pins_dir: format!("{CA}/pins"),
        endpoints: Endpoints {
            msb_url: MSB_API_URL.into(),
            msb_rate_url: MSB_RATE_URL.into(),
            box_admin: BOX_ADMIN.into(),
            // hems-mock dialt die Box im Compose-Netz (nicht über den Host-Publish)
            box_ship: "box-mock:19543".into(),
            hems_url: HEMS_URL.into(),
        },
        compose: Some(root.to_path_buf()),
    })
}

/// Restart the stateful docker services so every scenario starts from a clean slate.
fn compose_restart(root: &std::path::Path, services: &[&str]) -> Result<()> {
    let mut args: Vec<&str> = vec!["restart", "-t", "1"];
    args.extend_from_slice(services);
    run_docker(root, &args)?;
    Ok(())
}

fn run_docker(root: &std::path::Path, args: &[&str]) -> Result<()> {
    let mut cmd = std::process::Command::new("docker");
    cmd.args(["compose", "-f", COMPOSE_FILE])
        .args(args)
        .current_dir(root);
    let out = cmd.output()?;
    anyhow::ensure!(
        out.status.success(),
        "docker compose {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[derive(Default)]
struct StackOpts {
    rate_limit: Option<u32>,
    pins_dir: Option<String>,
}

// --- helpers ------------------------------------------------------------------

fn mtls_client(cert: &str, key: &str) -> Result<reqwest::Client> {
    ems_backend::ClientConfig::mtls(&format!("{CA}/server-roots.pem"), cert, key)
        .build_http_client()
}

fn default_client() -> Result<reqwest::Client> {
    ems_backend::ClientConfig::mtls(
        &format!("{CA}/server-roots.pem"),
        &format!("{CA}/client-cert.pem"),
        &format!("{CA}/client-key.pem"),
    )
    .build_http_client()
}

async fn wait_evidence(stack: &Stack, timeout: Duration) -> Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(resp) = reqwest::get(format!("{}/evidence", stack.endpoints.hems_url)).await {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if v.get("box").is_some() {
                    return Ok(v);
                }
            }
        }
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!("hems did not observe the Steuerbox in time");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn box_status(stack: &Stack) -> Result<serde_json::Value> {
    Ok(
        reqwest::get(format!("{}/admin/status", stack.endpoints.box_admin))
            .await?
            .json()
            .await?,
    )
}

async fn run_ems_backend(stack: &Stack) -> Result<(String, String)> {
    let log_dir = &stack.log_dir;
    let mut cmd = if stack.compose.is_some() {
        // docker mode: ems-backend runs as one-shot service inside the compose network
        let mut c = Command::new("docker");
        c.args([
            "compose",
            "-f",
            COMPOSE_FILE,
            "run",
            "--rm",
            "-T",
            "ems-backend",
        ]);
        c.env("RUST_LOG", "info");
        c
    } else {
        let mut c = Command::new(binary_path("ems-backend"));
        c.current_dir(ROOT)
            .env("EMS_PLATFORM_URLS", MSB_API_URL)
            .env("EMS_ROOT_BUNDLE", format!("{CA}/server-roots.pem"))
            .env("EMS_CLIENT_CERT", format!("{CA}/client-cert.pem"))
            .env("EMS_CLIENT_KEY", format!("{CA}/client-key.pem"))
            .env("EMS_WELLKNOWN_URL", WK_URL)
            .env("EMS_HEMS_URL", HEMS_URL)
            .env("EMS_QR_KEY", format!("{CA}/ems-qr-key.pem"))
            .env("EMS_AUDIT_LOG", log_dir.join("audit.jsonl"))
            .env("EMS_CLIENT_CA_PIN", log_dir.join("client-ca.pin"))
            .kill_on_drop(true);
        c
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let mut out = String::new();
    use tokio::io::AsyncReadExt;
    let mut err_out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut out).await?;
    }
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut err_out).await;
    }
    let status = child.wait().await?;
    std::fs::write(log_dir.join("ems-backend.log"), format!("{out}\n{err_out}"))?;
    ensure!(status.success(), "ems-backend refused flow: {err_out}");
    if !err_out.trim().is_empty() {
        tracing::warn!(target: "ems-backend", "{err_out}");
    }
    // RESULT canonical=<serial> outcome=<Debug of ProvisionOutcome>
    let line = out
        .lines()
        .find(|l| l.starts_with("RESULT "))
        .ok_or_else(|| anyhow::anyhow!("ems-backend produced no RESULT line: {out}"))?;
    let canonical = line
        .split("canonical=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or_default()
        .to_string();
    let outcome = out
        .lines()
        .find(|l| l.starts_with("RESULT "))
        .and_then(|l| l.split("outcome=").nth(1))
        .unwrap_or_default()
        .to_string();
    Ok((canonical, outcome))
}

fn assert_eq_(a: impl std::fmt::Debug, b: impl std::fmt::Debug, what: &str) -> Result<()> {
    if format!("{a:?}") == format!("{b:?}") {
        Ok(())
    } else {
        anyhow::bail!("FAIL {what}: {a:?} != {b:?}")
    }
}

// --- scenarios ----------------------------------------------------------------

/// HappyPath_ZeroTouch — full zero-touch flow incl. closed-loop SHIP handshake.
async fn happy_path(stack: &Stack) -> Result<()> {
    let evidence = wait_evidence(stack, Duration::from_secs(30)).await?;
    assert_eq_(
        evidence["stb_found"].clone(),
        serde_json::json!(true),
        "stb_found",
    )?;

    let (canonical, outcome) = run_ems_backend(stack).await?;
    assert_eq_(canonical, SN_GOLDEN, "canonical serial from discovery")?;
    assert_eq_(outcome, "Accepted", "provisioning outcome")?;

    // MSB wrote onto the box via the admin channel (async — poll until it lands)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let status = loop {
        let status = box_status(stack).await?;
        if status["pairing"].is_object() {
            break status;
        }
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!("admin-channel write did not reach the box in time");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq_(
        status["pairing"]["ship_id"].as_str(),
        Some(HEMS_SHIP_ID_LOCAL),
        "pairing dataset written to box",
    )?;

    // closed loop: SHIP handshake with the provisioned SKI is accepted
    let probe: serde_json::Value = reqwest::Client::new()
        .post(format!("{}/ship-connect", stack.endpoints.hems_url))
        .json(&serde_json::json!({"addr": stack.endpoints.box_ship}))
        .send()
        .await?
        .json()
        .await?;
    assert_eq_(
        probe["accepted"].clone(),
        serde_json::json!(true),
        "SHIP handshake accepted",
    )?;

    let status = box_status(stack).await?;
    assert_eq_(
        status["ship"]["accepted"].clone(),
        serde_json::json!(true),
        "closed loop proven",
    )?;
    // The mock may expose its test pairing state, but no service/audit log may
    // echo the actual QR/SPSEC forwarded through the encrypted API.
    let qr = status["pairing"]["qr_info"].as_str().unwrap_or_default();
    ensure!(
        qr.contains("SPSEC:"),
        "test pairing must carry a security code"
    );
    let spsec = qr
        .split("SPSEC:")
        .nth(1)
        .and_then(|s| s.split(';').next())
        .unwrap_or_default();
    ensure!(!spsec.is_empty(), "security code must not be empty");
    if let Some(root) = &stack.compose {
        let output = std::process::Command::new("docker")
            .args(["compose", "-f", COMPOSE_FILE, "logs", "--no-color"])
            .current_dir(root)
            .output()?;
        ensure!(
            output.status.success(),
            "cannot collect Compose service logs"
        );
        std::fs::write(stack.log_dir.join("services.log"), output.stdout)?;
    }
    for entry in std::fs::read_dir(&stack.log_dir)? {
        let path = entry?.path();
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("log" | "jsonl")
        ) {
            let text = std::fs::read_to_string(&path)?;
            ensure!(
                !text.contains(qr) && !text.contains(spsec),
                "pairing secret leaked into a service/audit log"
            );
        }
    }
    Ok(())
}

/// SuffixVPID — `-suffix` queries resolve to the canonical portfolio serial.
async fn suffix_vpid(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    for query in ["SNTEST0001-X9", "i:51593_u:SNTEST0001-X9"] {
        let d = ems_backend::discover(&client, &stack.endpoints.msb_url, query).await;
        match d {
            ems_backend::Discovery::Found { canonical_serial } => {
                assert_eq_(canonical_serial, SN_GOLDEN, "canonical from suffixed VPID")?;
            }
            other => anyhow::bail!("FAIL SuffixVPID: expected Found, got {other:?}"),
        }
    }
    let urls = vec![
        stack.endpoints.msb_url.clone(),
        format!("{}/", stack.endpoints.msb_url),
        "https://127.0.0.1:19999".into(),
    ];
    let results = ems_backend::fan_out_discover(&client, &urls, SN_GOLDEN).await;
    ensure!(results.len() == 2, "fan-out must deduplicate platform URLs");
    ensure!(matches!(results[0].1, ems_backend::Discovery::Found { .. }));
    ensure!(matches!(results[1].1, ems_backend::Discovery::Unknown(_)));
    Ok(())
}

/// TxtSerial — TXT-record driven serial derivation through the normative parser.
async fn txt_serial(stack: &Stack) -> Result<()> {
    // stub mode injects the golden TXT (valueless `serial` key, serial inside id=VPID)
    let evidence = wait_evidence(stack, Duration::from_secs(10)).await?;
    assert_eq_(
        evidence["box"]["serial"].as_str(),
        Some(SN_GOLDEN),
        "parser derived serial from VPID",
    )?;
    let (canonical, outcome) = run_ems_backend(stack).await?;
    assert_eq_(canonical, SN_GOLDEN, "TxtSerial canonical")?;
    assert_eq_(outcome, "Accepted", "TxtSerial provisioning")?;
    Ok(())
}

/// A changed well-known CA must not be accepted silently after initial pinning.
async fn wellknown_trust(stack: &Stack) -> Result<()> {
    let (_, outcome) = run_ems_backend(stack).await?;
    assert_eq_(outcome, "Accepted", "first well-known observation")?;
    let pin = if stack.compose.is_some() {
        std::path::Path::new(ROOT)
            .join(CA)
            .join("backend-pins/client-ca.pin")
    } else {
        stack.log_dir.join("client-ca.pin")
    };
    let wrong_ca = std::fs::read(format!("{ROOT}/{CA}/client-ca-v2-cert.pem"))?;
    if let Some(root) = &stack.compose {
        let mut child = Command::new("docker")
            .args([
                "compose",
                "-f",
                COMPOSE_FILE,
                "run",
                "--rm",
                "-T",
                "--entrypoint",
                "sh",
                "ems-backend",
                "-c",
                "cat > /pins/client-ca.pin",
            ])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().unwrap().write_all(&wrong_ca).await?;
        ensure!(
            child.wait().await?.success(),
            "cannot seed Docker TOFU mismatch"
        );
    } else {
        std::fs::write(&pin, wrong_ca)?;
    }
    let error = run_ems_backend(stack)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("changed TOFU pin must refuse provisioning"))?;
    let pin_mismatch = error.to_string().contains("qualified TOFU pin");
    if let Some(root) = &stack.compose {
        run_docker(
            root,
            &[
                "run",
                "--rm",
                "-T",
                "--entrypoint",
                "sh",
                "ems-backend",
                "-c",
                "rm -f /pins/client-ca.pin",
            ],
        )?;
    } else {
        std::fs::remove_file(&pin)?;
    }
    ensure!(
        pin_mismatch,
        "client must refuse because of pin mismatch: {error}"
    );
    Ok(())
}

/// TriState_Unknown — transport failure is unknown, never not found (404).
async fn tri_state_unknown(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    let d = ems_backend::discover(&client, "https://127.0.0.1:19999", SN_GOLDEN).await;
    ensure!(
        matches!(d, ems_backend::Discovery::Unknown(_)),
        "FAIL TriState: dead MSB must be Unknown, got {d:?}"
    );
    // zero_touch must refuse to conclude when a platform is unknown
    let body = serde_json::json!({"pairing_dataset": {"ship_id":"x","ski":"y","qr_info":"SHIP;SKI:y;ID:h;FPH256:z;ENDSHIP;"},"box_evidence":null});
    let r = ems_backend::zero_touch(
        &client,
        &["https://127.0.0.1:19999".into()],
        SN_GOLDEN,
        &body,
    )
    .await;
    ensure!(
        r.is_err(),
        "FAIL TriState: zero_touch must refuse on Unknown"
    );
    let mixed = ems_backend::zero_touch(
        &client,
        &[
            stack.endpoints.msb_url.clone(),
            "https://127.0.0.1:19999".into(),
        ],
        SN_GOLDEN,
        &body,
    )
    .await;
    ensure!(
        mixed.is_err(),
        "one reachable hit must not hide an unknown platform"
    );
    ensure!(
        box_status(stack).await?["pairing"].is_null(),
        "unknown fan-out must not write pairing data"
    );
    Ok(())
}

/// BoxEvidenceMismatch — wrong box evidence → 422 box_mismatch, no silent ignore.
async fn box_evidence_mismatch(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    let body = serde_json::json!({
        "pairing_dataset": {"ship_id":"i:12345_u:HEMS1","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"},
        "box_evidence": {"ship_id": format!("i:51593_u:{SN_GOLDEN}"), "ski": "WRONGSKI"}
    });
    let resp = client
        .post(format!(
            "{}/v1/serials/{SN_GOLDEN}/provisioning",
            stack.endpoints.msb_url
        ))
        .json(&body)
        .send()
        .await?;
    assert_eq_(resp.status().as_u16(), 422, "status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("box_mismatch"), "error code")?;
    Ok(())
}

/// RogueClientCA — a client certificate from an unpinned CA is rejected at TLS level.
async fn rogue_client_ca(stack: &Stack) -> Result<()> {
    ensure!(
        matches!(
            ems_backend::discover(&default_client()?, &stack.endpoints.msb_url, SN_GOLDEN).await,
            ems_backend::Discovery::Found { .. }
        ),
        "trusted client must work before testing a rogue CA"
    );
    let anonymous = ems_backend::ClientConfig::server_tls(&format!("{CA}/server-roots.pem"))
        .build_http_client()?;
    ensure!(
        anonymous
            .get(format!(
                "{}/v1/serials/{SN_GOLDEN}",
                stack.endpoints.msb_url
            ))
            .send()
            .await
            .is_err(),
        "API without client identity must fail at TLS"
    );
    let rogue = mtls_client(
        &format!("{CA}/rogue-client-cert.pem"),
        &format!("{CA}/rogue-client-key.pem"),
    )?;
    let d = ems_backend::discover(&rogue, &stack.endpoints.msb_url, SN_GOLDEN).await;
    ensure!(
        matches!(d, ems_backend::Discovery::Unknown(_)),
        "FAIL RogueClientCA: must be Unknown (TLS rejected), got {d:?}"
    );
    Ok(())
}

/// KillSwitch — operator kills a serial: discovery 404, provisioning 404,
/// and the answer must not leak that the serial ever existed.
async fn kill_switch(stack: &Stack) -> Result<()> {
    let mut sock = TcpStream::connect(MSB_ADMIN).await?;
    sock.write_all(format!("{MSB_TOKEN}\nKILL {SN_GOLDEN}\n").as_bytes())
        .await?;

    let client = default_client()?;
    let d = ems_backend::discover(&client, &stack.endpoints.msb_url, SN_GOLDEN).await;
    ensure!(
        matches!(d, ems_backend::Discovery::NotFound),
        "FAIL KillSwitch: discovery must be not found (404), got {d:?}"
    );
    let body = serde_json::json!({
        "pairing_dataset": {"ship_id":"s","ski":"k","qr_info":"SHIP;SKI:k;ID:h;FPH256:f;ENDSHIP;"}
    });
    let resp = client
        .post(format!(
            "{}/v1/serials/{SN_GOLDEN}/provisioning",
            stack.endpoints.msb_url
        ))
        .json(&body)
        .send()
        .await?;
    assert_eq_(resp.status().as_u16(), 404, "provisioning after kill")?;
    // error taxonomy: the 404 body must carry the machine-readable not_found code
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("not_found"), "404 error code")?;
    // no existence leak: a NEVER-existing serial reads exactly the same
    let resp2 = client
        .post(format!(
            "{}/v1/serials/SNTEST0999/provisioning",
            stack.endpoints.msb_url
        ))
        .json(&body)
        .send()
        .await?; // SNTEST0999 never provisioned, never killed, not in portfolio → same 404
    assert_eq_(resp2.status().as_u16(), 404, "unknown serial provisioning")?;
    let err2: serde_json::Value = resp2.json().await?;
    assert_eq_(
        err2["error"].as_str(),
        err["error"].as_str(),
        "killed and unknown serial must be indistinguishable",
    )?;
    Ok(())
}

/// ClientCertRotation — a rotated leaf under the SAME client CA keeps working.
async fn client_cert_rotation(stack: &Stack) -> Result<()> {
    let client = mtls_client(
        &format!("{CA}/client-b-cert.pem"),
        &format!("{CA}/client-b-key.pem"),
    )?;
    let d = ems_backend::discover(&client, &stack.endpoints.msb_url, SN_GOLDEN).await;
    ensure!(
        matches!(d, ems_backend::Discovery::Found { .. }),
        "FAIL ClientCertRotation: rotated leaf rejected: {d:?}"
    );
    Ok(())
}

/// ClientCARotation — pinning the new client CA (grace window: both pinned) lets the
/// rotated CA's clients through WITHOUT restarting the MSB (pin store is watched).
async fn client_ca_rotation(stack: &Stack) -> Result<()> {
    let unpinned = mtls_client(
        &format!("{CA}/client-v2-cert.pem"),
        &format!("{CA}/client-v2-key.pem"),
    )?;
    ensure!(
        matches!(
            ems_backend::discover(&unpinned, &stack.endpoints.msb_url, SN_GOLDEN).await,
            ems_backend::Discovery::Unknown(_)
        ),
        "new CA must be rejected before pinning"
    );
    // simulate CA rotation: new CA appears in the pin store
    std::fs::copy(
        format!("{ROOT}/{CA}/client-ca-v2-cert.pem"),
        format!("{}/client-ca-v2.pem", stack.pins_dir),
    )?;

    // pin-store reload is lazy (checked per new connection) — retry until visible
    let client = mtls_client(
        &format!("{CA}/client-v2-cert.pem"),
        &format!("{CA}/client-v2-key.pem"),
    )?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let new_ca_ok = loop {
        let attempt = ems_backend::discover(&client, &stack.endpoints.msb_url, SN_GOLDEN).await;
        if matches!(attempt, ems_backend::Discovery::Found { .. }) {
            break attempt;
        }
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!(
                "FAIL ClientCARotation: new-CA client rejected after grace wait: {attempt:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    // old CA still works during the grace window
    let old = default_client()?;
    let d = ems_backend::discover(&old, &stack.endpoints.msb_url, SN_GOLDEN).await;
    ensure!(
        matches!(d, ems_backend::Discovery::Found { .. })
            && matches!(new_ca_ok, ems_backend::Discovery::Found { .. }),
        "FAIL ClientCARotation: new/old CA rejected during grace: new={new_ca_ok:?} old={d:?}"
    );
    std::fs::remove_file(format!("{}/client-ca.pem", stack.pins_dir))?;
    // A fresh client forces a new TLS connection; existing sessions are not revoked.
    ensure!(
        matches!(
            ems_backend::discover(&default_client()?, &stack.endpoints.msb_url, SN_GOLDEN).await,
            ems_backend::Discovery::Unknown(_)
        ),
        "old CA must be rejected after grace"
    );
    ensure!(
        matches!(
            ems_backend::discover(
                &mtls_client(
                    &format!("{CA}/client-v2-cert.pem"),
                    &format!("{CA}/client-v2-key.pem")
                )?,
                &stack.endpoints.msb_url,
                SN_GOLDEN
            )
            .await,
            ems_backend::Discovery::Found { .. }
        ),
        "new CA must remain accepted after grace"
    );
    // Reset the shared test PKI for subsequent independent scenarios.
    std::fs::copy(
        format!("{ROOT}/{CA}/client-ca-cert.pem"),
        format!("{}/client-ca.pem", stack.pins_dir),
    )?;
    std::fs::remove_file(format!("{}/client-ca-v2.pem", stack.pins_dir))?;
    Ok(())
}

/// RateLimit — 429 + Retry-After is contract; the reference client honors it.
async fn rate_limit(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    for _ in 0..10_000 {
        let d = ems_backend::discover(&client, &stack.endpoints.msb_rate_url, "SNTEST0009").await;
        match d {
            ems_backend::Discovery::NotFound => {}
            ems_backend::Discovery::Unknown(_why) => {
                // this is the 429 path: non-200 → Unknown. Verify header on a raw call.
                let raw = client
                    .get(format!(
                        "{}/v1/serials/{SN_GOLDEN}",
                        stack.endpoints.msb_rate_url
                    ))
                    .send()
                    .await?;
                assert_eq_(raw.status().as_u16(), 429, "rate limit status")?;
                let retry: u64 = raw
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_default();
                let err: serde_json::Value = raw.json().await?;
                assert_eq_(
                    err["error"].as_str(),
                    Some("rate_limited"),
                    "429 body must carry the machine-readable rate_limited code",
                )?;
                assert!(
                    (1..=15).contains(&retry),
                    "FAIL RateLimit: Retry-After must be truthful, got {retry}"
                );
                tokio::time::sleep(Duration::from_secs(retry)).await; // honor Retry-After
                let d = ems_backend::discover(&client, &stack.endpoints.msb_rate_url, "SNTEST0009")
                    .await;
                ensure!(
                    matches!(d, ems_backend::Discovery::NotFound),
                    "FAIL RateLimit: after Retry-After the quota must reset, got {d:?}"
                );
                // provisioning honors 429 + Retry-After on the same quota window:
                // quota is still exhausted → the reference client must sleep and
                // eventually reach the (reset) endpoint instead of failing.
                let body = serde_json::json!({
                    "pairing_dataset": {"ship_id":"i:12345_u:HEMS1","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"}
                });
                // Exhaust the reset window so provision() really takes its 429
                // sleep/retry path; an unthrottled POST would not prove backoff.
                for _ in 0..2 {
                    ensure!(matches!(
                        ems_backend::discover(&client, &stack.endpoints.msb_rate_url, "SNTEST0009")
                            .await,
                        ems_backend::Discovery::NotFound
                    ));
                }
                let blocked = client
                    .post(format!(
                        "{}/v1/serials/{SN_GOLDEN}/provisioning",
                        stack.endpoints.msb_rate_url
                    ))
                    .json(&body)
                    .send()
                    .await?;
                ensure!(blocked.status().as_u16() == 429);
                let retry = ems_backend::retry_after_from(blocked.headers())
                    .ok_or_else(|| anyhow::anyhow!("missing Retry-After"))?;
                let started = std::time::Instant::now();
                let outcome = ems_backend::provision(
                    &client,
                    &stack.endpoints.msb_rate_url,
                    SN_GOLDEN,
                    &body,
                )
                .await;
                ensure!(
                    matches!(outcome, ems_backend::ProvisionOutcome::Accepted),
                    "FAIL RateLimit: provisioning must honor Retry-After (sleep+retry), got {outcome:?}"
                );
                ensure!(
                    started.elapsed() >= Duration::from_secs(retry),
                    "client retried before Retry-After"
                );
                return Ok(());
            }
            other => anyhow::bail!("FAIL RateLimit: unexpected {other:?}"),
        }
    }
    anyhow::bail!("FAIL RateLimit: quota never exhausted")
}

/// IdempotentPost — idempotent on (serial, ship_id); different ship_id → 409 write-once.
async fn idempotent_post(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    let body = serde_json::json!({
        "pairing_dataset": {"ship_id":"i:12345_u:HEMS1","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"}
    });
    let url = format!(
        "{}/v1/serials/{SN_SECOND}/provisioning",
        stack.endpoints.msb_url
    );
    let r1 = client.post(&url).json(&body).send().await?;
    assert_eq_(r1.status().as_u16(), 202, "first provision")?;
    let r2 = client.post(&url).json(&body).send().await?;
    assert_eq_(r2.status().as_u16(), 202, "idempotent repeat")?;
    let conflict = serde_json::json!({
        "pairing_dataset": {"ship_id":"i:12345_u:HEMS-OTHER","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id-2;FPH256:fph;ENDSHIP;"}
    });
    let r3 = client.post(&url).json(&conflict).send().await?;
    assert_eq_(r3.status().as_u16(), 409, "write-once conflict")?;
    Ok(())
}

/// FleetMismatch — ADR-0018: the IANA PEN inside pairing_dataset.ship_id must match
/// the iana_pen the MSB pinned from the well-known document; foreign-fleet payloads
/// are 422 fleet_mismatch + operator alarm, never a silent ignore.
async fn fleet_mismatch(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    let url = format!(
        "{}/v1/serials/{SN_GOLDEN}/provisioning",
        stack.endpoints.msb_url
    );
    // foreign PEN (a PEN the MSB did not pin) → 422 fleet_mismatch
    let foreign = serde_json::json!({
        "pairing_dataset": {"ship_id":"i:99999_u:HEMS-FOREIGN","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"}
    });
    let resp = client.post(&url).json(&foreign).send().await?;
    assert_eq_(resp.status().as_u16(), 422, "foreign PEN status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("fleet_mismatch"), "error code")?;

    // ship_id outside the SHIP-ID shape (no PEN) → also fleet_mismatch
    let malformed = serde_json::json!({
        "pairing_dataset": {"ship_id":"ship-without-pen","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"}
    });
    let resp = client.post(&url).json(&malformed).send().await?;
    assert_eq_(resp.status().as_u16(), 422, "missing PEN status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("fleet_mismatch"), "error code")?;

    // own fleet PEN → accepted (202)
    let own = serde_json::json!({
        "pairing_dataset": {"ship_id": format!("i:{PEN_EMS}_u:HEMS-FLEET"),"ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"}
    });
    let resp = client.post(&url).json(&own).send().await?;
    assert_eq_(
        resp.status().as_u16(),
        202,
        "own-fleet PEN must be accepted",
    )?;
    Ok(())
}

/// AntiEnumeration — shorter queries and unknown serials return 404.
async fn anti_enumeration(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    for q in ["SNTE", "SNTEST000", "SNTEST9999"] {
        let d = ems_backend::discover(&client, &stack.endpoints.msb_url, q).await;
        ensure!(
            matches!(d, ems_backend::Discovery::NotFound),
            "FAIL AntiEnumeration: {q} must be 404, got {d:?}"
        );
    }
    Ok(())
}

/// ErrorContract — the machine-readable error taxonomy of spec/provisioning.yaml:
/// 400 not_canonical, 404 not_found (unknown serial), 422 invalid_pairing_dataset,
/// 422 invalid_body. (409 write_once_conflict is normative in IdempotentPost,
/// 422 box_mismatch in BoxEvidenceMismatch.)
async fn error_contract(stack: &Stack) -> Result<()> {
    let client = default_client()?;
    let valid_dataset = serde_json::json!({
        "pairing_dataset": {"ship_id":"i:12345_u:HEMS1","ski":"ski-hems","qr_info":"SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"}
    });

    // 400 not_canonical: a client-derived (hyphenated, Regel-S non-base) serial
    let resp = client
        .post(format!(
            "{}/v1/serials/{SN_GOLDEN}-X9/provisioning",
            stack.endpoints.msb_url
        ))
        .json(&valid_dataset)
        .send()
        .await?;
    assert_eq_(resp.status().as_u16(), 400, "non-canonical serial status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("not_canonical"), "error code")?;

    // 404 not_found: unknown serial (not in portfolio, not killed) — no 202 write
    let resp = client
        .post(format!(
            "{}/v1/serials/SNTEST0999/provisioning",
            stack.endpoints.msb_url
        ))
        .json(&valid_dataset)
        .send()
        .await?;
    assert_eq_(resp.status().as_u16(), 404, "unknown serial status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("not_found"), "error code")?;

    // 422 invalid_pairing_dataset: qr_info outside ^SHIP;.+ENDSHIP;$ (ADR-0017)
    let bad = serde_json::json!({
        "pairing_dataset": {"ship_id":"i:12345_u:HEMS1","ski":"ski-hems","qr_info":"SHIP;SKI:x;"} // no ENDSHIP → 422
    });
    let resp = client
        .post(format!(
            "{}/v1/serials/{SN_GOLDEN}/provisioning",
            stack.endpoints.msb_url
        ))
        .json(&bad)
        .send()
        .await?;
    assert_eq_(resp.status().as_u16(), 422, "bad HEMS serial status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(
        err["error"].as_str(),
        Some("invalid_pairing_dataset"),
        "error code",
    )?;

    // 422 invalid_body: not valid JSON
    let resp = client
        .post(format!(
            "{}/v1/serials/{SN_GOLDEN}/provisioning",
            stack.endpoints.msb_url
        ))
        .header("content-type", "application/json")
        .body("not-json")
        .send()
        .await?;
    assert_eq_(resp.status().as_u16(), 422, "invalid body status")?;
    let err: serde_json::Value = resp.json().await?;
    assert_eq_(err["error"].as_str(), Some("invalid_body"), "error code")?;

    // the reference client must surface the 422 taxonomy instead of collapsing
    // every 422 into box_mismatch
    let outcome = ems_backend::provision(&client, &stack.endpoints.msb_url, SN_GOLDEN, &bad).await;
    assert_eq_(
        outcome,
        ems_backend::ProvisionOutcome::Unprocessable("invalid_pairing_dataset".into()),
        "client 422 taxonomy",
    )?;
    Ok(())
}

/// NetworkBoundaries — only the MSB writes onto the box; HEMS never holds the token;
/// a wrong SKI never becomes a coupling.
async fn network_boundaries(stack: &Stack) -> Result<()> {
    if let Some(root) = &stack.compose {
        run_docker(root, &["exec", "-T", "hems-mock", "sh", "-c",
            "test ! -e /certs/client-key.pem && test ! -e /certs/msb-key.pem && test ! -e /certs/ems-qr-key.pem"])?;
    }
    happy_path(stack).await?;
    // (a) admin channel is token-gated: no token → 401
    let r = reqwest::Client::new()
        .post(format!(
            "{}/admin/pairing/{SN_GOLDEN}",
            stack.endpoints.box_admin
        ))
        .json(&serde_json::json!({"ship_id":"evil","ski":"evil"}))
        .send()
        .await?;
    assert_eq_(r.status().as_u16(), 401, "admin write without token")?;
    // (b) wrong token → 401
    let r = reqwest::Client::new()
        .post(format!(
            "{}/admin/pairing/{SN_GOLDEN}",
            stack.endpoints.box_admin
        ))
        .header("x-admin-token", "wrong-token")
        .json(&serde_json::json!({"ship_id":"evil","ski":"evil"}))
        .send()
        .await?;
    assert_eq_(r.status().as_u16(), 401, "admin write with wrong token")?;
    // (c) hems evidence must not leak MSB credentials or the admin token
    let evidence = wait_evidence(stack, Duration::from_secs(10)).await?;
    let raw = serde_json::to_string(&evidence)?;
    ensure!(
        !raw.contains(BOX_TOKEN),
        "FAIL: HEMS evidence leaks admin token"
    );
    ensure!(
        !raw.contains("18443"),
        "FAIL: HEMS evidence leaks MSB coordinates"
    );
    // (d) wrong SKI must NOT couple (never trust LAN claims without the closed loop)
    let _ = box_status(stack).await?;
    let probe: serde_json::Value = reqwest::Client::new()
        .post(format!("{}/ship-connect", stack.endpoints.hems_url))
        .json(&serde_json::json!({"addr": stack.endpoints.box_ship, "ski": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"}))
        .send()
        .await?
        .json()
        .await?;
    assert_eq_(
        probe["accepted"].clone(),
        serde_json::json!(false),
        "wrong SKI rejected",
    )?;
    let status = box_status(stack).await?;
    assert_eq_(
        status["ship"]["accepted"].clone(),
        serde_json::json!(false),
        "wrong SKI cannot couple even after valid provisioning",
    )?;
    let _ = stack; // stack keeps services alive for the assertions above
    Ok(())
}

// --- runner -------------------------------------------------------------------

fn scenarios() -> Vec<&'static str> {
    vec![
        "HappyPath_ZeroTouch",
        "SuffixVPID",
        "TxtSerial",
        "WellKnownTrust",
        "TriState_Unknown",
        "BoxEvidenceMismatch",
        "RogueClientCA",
        "KillSwitch",
        "ClientCertRotation",
        "ClientCARotation",
        "RateLimit",
        "IdempotentPost",
        "AntiEnumeration",
        "ErrorContract",
        "FleetMismatch",
        "NetworkBoundaries",
    ]
}

/// Run every black-box conformance test against the same fresh local stack.
async fn conformance(stack: &Stack) -> Result<()> {
    let status = Command::new(std::env::var("PYTHON").unwrap_or_else(|_| "python3".into()))
        .current_dir(ROOT)
        .args([
            "-m",
            "pytest",
            "conformance/tests",
            "-v",
            "--target",
            &stack.endpoints.msb_url,
        ])
        .env("CONF_CERT", format!("{ROOT}/{CA}/client-cert.pem"))
        .env("CONF_KEY", format!("{ROOT}/{CA}/client-key.pem"))
        .env("CONF_ROOT", format!("{ROOT}/{CA}/server-roots.pem"))
        .env("CONF_SERIAL", SN_GOLDEN)
        .env("CONF_IANA_PEN", PEN_EMS)
        .env("CONF_RATE_TARGET", &stack.endpoints.msb_rate_url)
        .kill_on_drop(true)
        .status()
        .await?;
    ensure!(status.success(), "conformance failed: {status}");
    Ok(())
}

async fn run_scenario(name: &str, stack: &Stack) -> Result<()> {
    match name {
        "HappyPath_ZeroTouch" => happy_path(stack).await,
        "SuffixVPID" => suffix_vpid(stack).await,
        "TxtSerial" => txt_serial(stack).await,
        "WellKnownTrust" => wellknown_trust(stack).await,
        "TriState_Unknown" => tri_state_unknown(stack).await,
        "BoxEvidenceMismatch" => box_evidence_mismatch(stack).await,
        "RogueClientCA" => rogue_client_ca(stack).await,
        "KillSwitch" => kill_switch(stack).await,
        "ClientCertRotation" => client_cert_rotation(stack).await,
        "ClientCARotation" => client_ca_rotation(stack).await,
        "RateLimit" => rate_limit(stack).await,
        "IdempotentPost" => idempotent_post(stack).await,
        "AntiEnumeration" => anti_enumeration(stack).await,
        "ErrorContract" => error_contract(stack).await,
        "FleetMismatch" => fleet_mismatch(stack).await,
        "NetworkBoundaries" => network_boundaries(stack).await,
        "Conformance" => conformance(stack).await,
        other => anyhow::bail!("unknown scenario {other}"),
    }
}

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt().init();

    // CA must exist (make test-ca)
    for f in [
        "msb-chain.pem",
        "client-cert.pem",
        "server-roots.pem",
        "ems-qr-key.pem",
    ] {
        let p = format!("{ROOT}/{CA}/{f}");
        if !std::path::Path::new(&p).exists() {
            anyhow::bail!("missing {p} — run `make test-ca` first");
        }
    }

    let only = std::env::var("E2E_SCENARIO").ok();
    let selected = match only.as_deref() {
        Some("Conformance") => vec!["Conformance"],
        Some(name) if !scenarios().contains(&name) => anyhow::bail!("unknown scenario {name}"),
        _ => scenarios(),
    };
    // Stack mode: local processes (default, `E2E_STACK=local`) or docker services
    // (`E2E_STACK=docker`, deploy/compose — backend-to-backend as containers).
    let docker_mode = std::env::var("E2E_STACK").as_deref() == Ok("docker");
    tracing::info!(
        cwd = ?std::env::current_dir(),
        docker_mode,
        "stack mode: {}",
        if docker_mode { "docker (deploy/compose)" } else { "local processes" }
    );
    let log_dir = std::path::Path::new(ROOT).join("reference/target/e2e-logs");
    std::fs::create_dir_all(&log_dir)?;

    let mut passed: Vec<&str> = vec![];
    let mut failed: Vec<(&str, String)> = vec![];

    if docker_mode {
        let stack = start_stack_docker(
            std::path::Path::new(ROOT),
            &[
                MSB_API,
                WK_BIND,
                "127.0.0.1:18642",
                HEMS_BIND,
                "127.0.0.1:18453",
            ],
        )
        .await?;
        for name in selected {
            if let Some(o) = &only {
                if o.as_str() != name {
                    continue;
                }
            }
            print!("{name:<22} ");
            // reset stateful services so every scenario starts from a clean slate
            compose_restart(
                std::path::Path::new(ROOT),
                &["msb", "msb-ratelimit", "box-mock", "hems-mock"],
            )?;
            for port in [MSB_API, "127.0.0.1:18642", HEMS_BIND] {
                wait_port(port, Duration::from_secs(60)).await?;
            }
            match run_scenario(name, &stack).await {
                Ok(()) => {
                    println!("PASS");
                    passed.push(name);
                }
                Err(e) => {
                    println!("FAIL: {e:#}");
                    failed.push((name, e.to_string()));
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        drop(stack); // docker compose down
    } else {
        for name in selected {
            if let Some(o) = &only {
                if o.as_str() != name {
                    continue;
                }
            }
            print!("{name:<22} ");
            // per-scenario log dir so failed scenarios keep their own service logs
            let stack_log_dir = log_dir.join(name);
            std::fs::create_dir_all(&stack_log_dir)?;
            match start_stack_local(&stack_log_dir, StackOpts::default()).await {
                Ok(stack) => match run_scenario(name, &stack).await {
                    Ok(()) => {
                        println!("PASS");
                        passed.push(name);
                    }
                    Err(e) => {
                        println!("FAIL: {e:#}");
                        failed.push((name, e.to_string()));
                    }
                },
                Err(e) => {
                    println!("ERROR (stack: {e})");
                    failed.push((name, format!("stack: {e}")));
                    continue;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await; // ports settle
        }
    }

    println!("\n{} passed, {} failed", passed.len(), failed.len());
    for (name, why) in &failed {
        println!("  FAIL {name}: {why}");
    }
    if !failed.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
