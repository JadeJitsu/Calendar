//! Live CalDAV discovery probe (debug aid).
//!
//! Drives the real `CalDavClient` discovery chain (the same code the app
//! runs) and prints the status of each step. Usage:
//!   cargo run --bin caldav_probe -- <url> <user> <pass>
//!
//! Prints only hosts, URLs, status codes, and the server's own error
//! message — never the password or event data.
#[path = "../caldav.rs"]
mod caldav;
use caldav::CalDavClient;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: caldav_probe <url> <user> <pass>");
        std::process::exit(2);
    }
    let (url, user, pass) = (&args[1], &args[2], &args[3]);
    let client = match CalDavClient::new(url.clone(), user.clone(), pass.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("client: {e}");
            std::process::exit(1);
        }
    };

    match client.discover_principal() {
        Ok(p) => println!("step A (principal): OK -> {p}"),
        Err(e) => {
            println!("step A (principal): ERR {e}");
            std::process::exit(1);
        }
    }
    let principal = client.discover_principal().expect("ok above");
    match client.discover_home(&principal) {
        Ok(h) => println!("step B (home-set): OK -> {h}"),
        Err(e) => {
            println!("step B (home-set): ERR {e}");
            // Dump the raw response so we can see what the server actually
            // returned. Contains only URLs/properties — no credentials or
            // event data.
            let http = reqwest::blocking::Client::new();
            let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:calendar-home-set/></d:prop></d:propfind>"#;
            let method = reqwest::Method::from_bytes(b"PROPFIND").unwrap();
            let resp = http
                .request(method, &principal)
                .header("Depth", "0")
                .header("Content-Type", "application/xml; charset=utf-8")
                .basic_auth(user, Some(pass))
                .body(body.to_string())
                .send()
                .unwrap();
            println!("  raw response (status {}):", resp.status());
            println!("{}", resp.text().unwrap_or_default());
            std::process::exit(1);
        }
    }
    let home = client.discover_home(&principal).expect("ok above");
    match client.list_calendars(&home) {
        Ok(cals) => {
            println!("step C (list): OK, {} calendar(s)", cals.len());
            for c in &cals {
                println!("  - {} : {}", c.name, c.url);
            }
        }
        Err(e) => {
            println!("step C (list): ERR {e}");
            std::process::exit(1);
        }
    }
}
