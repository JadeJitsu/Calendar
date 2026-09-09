//! Live CalDAV REPORT probe: fetch events from a collection URL directly,
//! bypassing principal/home-set discovery (the way Thunderbird works when
//! given a direct collection URL). Usage:
//!   cargo run --bin report_probe -- <collection_url> <user> <pass>
//! Prints the event count and UIDs only — never event bodies.
use caldav::CalDavClient;

#[path = "../caldav.rs"]
mod caldav;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: report_probe <collection_url> <user> <pass>");
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
    match client.fetch_events(url) {
        Ok(pairs) => {
            println!("REPORT OK: {} event resource(s)", pairs.len());
            for (href, ics) in &pairs {
                // Extract UID lines only (safe to print).
                let uids: Vec<&str> = ics
                    .lines()
                    .filter(|l| l.to_uppercase().starts_with("UID:"))
                    .collect();
                println!("  {}  uid={:?}", href, uids.first().copied().unwrap_or("<none>"));
            }
        }
        Err(e) => {
            println!("REPORT ERR: {e}");
            std::process::exit(1);
        }
    }
}
