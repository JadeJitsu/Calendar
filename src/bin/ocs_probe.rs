//! Live Nextcloud OCS Calendar API probe (debug aid).
//!
//! The OCS Calendar API is independent of the DAV layer, so it works even
//! when the server's CalDAV (SabreDAV) plugin is broken. Usage:
//!   cargo run --bin ocs_probe -- <server_root> <user> <pass>
//! where <server_root> is e.g. `https://c125.lv.tabdigital.eu` (no path).
//!
//! Dumps the OCS calendars list and one events fetch so the real response
//! shapes can be captured. Prints only structure — truncates event bodies.
use reqwest::blocking::Client;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: ocs_probe <server_root> <user> <pass>");
        std::process::exit(2);
    }
    let (root, user, pass) = (&args[1], &args[2], &args[3]);
    let root = root.trim_end_matches('/');
    let client = Client::builder().https_only(true).build().expect("client");

    // 1. List calendars.
    let list_url = format!("{root}/ocs/v2.php/calendars/{user}/calendars");
    let resp = client
        .get(&list_url)
        .header("OCS-APIRequest", "true")
        .header("Accept", "application/json")
        .basic_auth(user, Some(pass))
        .send()
        .expect("send");
    let status = resp.status();
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?")
        .to_string();
    let body = resp.text().unwrap_or_default();
    println!("=== GET {list_url}");
    println!("status: {status}  content-type: {ctype}");
    println!(
        "body (first 2000 chars):\n{}",
        body.chars().take(2000).collect::<String>()
    );

    // 2. If the list is OCS-wrapped JSON, grab the first calendar id/uri and
    //    fetch its events to capture that shape too.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(cal) = json["ocs"]["data"].as_array().and_then(|a| a.first()) {
            let id = cal
                .get("id")
                .and_then(|v| v.as_i64())
                .map(|v| v.to_string())
                .or_else(|| {
                    cal.get("uri")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let uri = cal.get("uri").and_then(|v| v.as_str()).unwrap_or("?");
            println!("\nfirst calendar: id={id} uri={uri}");
            let ev_url = format!("{root}/ocs/v2.php/calendars/{user}/calendars/{id}/events");
            let resp = client
                .get(&ev_url)
                .header("OCS-APIRequest", "true")
                .basic_auth(user, Some(pass))
                .send()
                .expect("send");
            let status = resp.status();
            let ctype = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?")
                .to_string();
            let body = resp.text().unwrap_or_default();
            println!("=== GET {ev_url}");
            println!("status: {status}  content-type: {ctype}");
            println!(
                "body (first 1200 chars):\n{}",
                body.chars().take(1200).collect::<String>()
            );
        }
    }
}
