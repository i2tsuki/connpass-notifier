extern crate imap;
extern crate native_tls;

use std::env;

fn main() {
    let domain = env::var("IMAP_DOMAIN").unwrap();
    let port = env::var("IMAP_PORT").unwrap().parse::<u16>().unwrap();
    let user = env::var("IMAP_USER").unwrap();
    let password = env::var("IMAP_PASSWORD").unwrap();

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = imap::connect((domain.as_str(), port), &domain, &tls).unwrap();

    let mut imap_session = client.login(user, password).unwrap();

    println!("Hello, world!");
}
