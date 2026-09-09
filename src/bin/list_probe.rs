//! Live CalDAV collection lister: PROPFIND Depth 1 against a URL and print
//! the child collections. Usage:
//!   cargo run --bin list_probe -- <url> <user> <pass>
use caldav::CalDavClient;

#[path = "../caldav.rs"]
mod caldav;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: list_probe <url> <user> <pass>");
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
    // list_calendars PROPFINDs the given URL at Depth 1 and parses the
    // calendar collections from the response.
    match client.list_calendars(url) {
        Ok(cals) => {
            println!("{} collection(s) under {url}:", cals.len());
            for c in &cals {
                println!("  - {} : {}", c.name, c.url);
            }
        }
        Err(e) => {
            println!("LIST ERR: {e}");
            std::process::exit(1);
        }
    }
}
