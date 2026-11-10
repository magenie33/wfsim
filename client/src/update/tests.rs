// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

fn fixture(body: Value) -> (Signed, String) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let body = serde_json::to_string(&body).unwrap();
    let sig = key
        .sign(body.as_bytes())
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let public = key
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    (Signed { body, sig }, public)
}

fn manifest(version: &str, bytes: &[u8]) -> Value {
    json!({"v":1,"launch":1,"version":version,"npm":true,"files":{target():{
        "name":format!("wfsim-{}{}",target(),if cfg!(windows){".exe"}else{""}),
        "size":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes)),"urls":[]}}})
}

#[test]
fn signed_host_binds_the_launch_contract_platform_and_raw_bytes() {
    let dir = Path::new("unused-device");
    let (signed, key) = fixture(manifest("0.2.1", b"fixture"));
    assert!(Host::parse(dir, signed.clone(), &[&key]).is_ok());
    let mut bad = signed.clone();
    bad.body.push(' ');
    assert!(Host::parse(dir, bad, &[&key]).is_err());
    assert!(Host::parse(dir, signed.clone(), trust::PUBLIC_KEYS).is_err());
    let mut body: Value = serde_json::from_str(&signed.body).unwrap();
    body["launch"] = json!(2);
    let (bad, _) = fixture(body.clone());
    assert!(Host::parse(dir, bad, &[&key]).is_err());
    body["launch"] = json!(1);
    body["files"] = json!({});
    let (bad, _) = fixture(body);
    assert!(Host::parse(dir, bad, &[&key]).is_err());
    let mut body = manifest("0.2.1", b"fixture");
    body["files"][target()]["name"] = json!("wrong-platform");
    let (bad, _) = fixture(body);
    assert!(Host::parse(dir, bad, &[&key]).is_err());
    for invalid in ["../0.2.1", "v0.2.1", "0.2.1+unbound", "01.2.1"] {
        let (bad, _) = fixture(manifest(invalid, b"fixture"));
        assert!(Host::parse(dir, bad, &[&key]).is_err());
    }
}

#[test]
fn interrupted_downloads_and_corrupted_hosts_never_become_current() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("wfsim-host-{}", state::random_hex(8)?));
    state::directory(&dir)?;
    let (signed, key) = fixture(manifest("0.2.1", b"complete bytes"));
    let host = Host::parse(&dir, signed.clone(), &[&key])?;
    state::directory(host.path.parent().unwrap())?;
    fs::write(host.path.with_extension("download"), b"partial")?;
    assert!(!host.valid());
    let state = Installed {
        current: Some(signed),
        previous: None,
        highest: Some("0.2.1".into()),
    };
    save(&dir, &state)?;
    assert!(current(&dir, &installed(&dir)?, &[&key]).is_none());
    fs::write(&host.path, b"complete bytes")?;
    assert!(current(&dir, &installed(&dir)?, &[&key]).is_some());
    fs::write(&host.path, b"changed bytes")?;
    assert!(current(&dir, &installed(&dir)?, &[&key]).is_none());
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn an_old_unsigned_pointer_is_only_a_locator() -> Result<()> {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let server = TcpListener::bind("127.0.0.1:0")?;
    let source = format!("http://{}", server.local_addr()?);
    let reply = std::thread::spawn(move || {
        let (mut client, _) = server.accept().unwrap();
        let mut request = [0; 1024];
        assert!(client.read(&mut request).unwrap() > 0);
        let body = r#"{"v":1,"version":"0.1.0","files":{}}"#;
        write!(
            client,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    assert!(check(
        Path::new("unused-device"),
        &Version::new(0, 2, 0),
        &[source],
        trust::PUBLIC_KEYS
    )?
    .is_none());
    reply.join().unwrap();
    Ok(())
}

#[test]
fn bad_downloads_and_pointer_substitution_leave_the_old_state() -> Result<()> {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    for substituted in [false, true] {
        let dir = std::env::temp_dir().join(format!("wfsim-host-refuse-{}", state::random_hex(8)?));
        let server = TcpListener::bind("127.0.0.1:0")?;
        let source = format!("http://{}", server.local_addr()?);
        let mut body = manifest(
            if substituted { "0.2.2" } else { "0.2.1" },
            b"expected payload",
        );
        body["files"][target()]["urls"] = json!([format!("{source}/native")]);
        let (signed, key) = fixture(body);
        let reply = std::thread::spawn(move || {
            for _ in 0..if substituted { 3 } else { 4 } {
                let (mut client, _) = server.accept().unwrap();
                let mut request = [0; 4096];
                let n = client.read(&mut request).unwrap();
                let path = String::from_utf8_lossy(&request[..n])
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_string();
                let response = if path.ends_with("latest.json") {
                    r#"{"version":"0.2.1"}"#.into()
                } else if path.ends_with(".sig") {
                    signed.sig.clone()
                } else if path.ends_with("release.json") {
                    signed.body.clone()
                } else {
                    "corrupt payload".into()
                };
                write!(
                    client,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
            }
        });
        assert!(check(&dir, &Version::new(0, 2, 0), &[source], &[&key]).is_err());
        assert!(!dir.join("hosts/state.json").exists());
        reply.join().unwrap();
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}
