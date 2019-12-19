extern crate imap;
extern crate native_tls;

use std::env;
use std::io::{Read, Write};

use imap::types::Seq;
use mailparse::*;

fn main() {
    let domain = env::var("IMAP_DOMAIN").unwrap();
    let port = env::var("IMAP_PORT").unwrap().parse::<u16>().unwrap();
    let user = env::var("IMAP_USER").unwrap();
    let password = env::var("IMAP_PASSWORD").unwrap();

    let chunk = 10;

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = imap::connect((domain.as_str(), port), &domain, &tls).unwrap();

    let mut imap_session = client.login(user, password).unwrap();

    imap_session.select("INBOX").unwrap();

    let sequences = imap_session.search("FROM no-reply@connpass.com").unwrap();
    for (i, seq) in sequences.iter().enumerate() {
        get_message_subject(&mut imap_session, *seq);
        if i > chunk {
            break;
        }
    }

    imap_session.logout().unwrap();
}

fn get_message_subject<T: Read + Write>(imap_session: &mut imap::Session<T>, seq: Seq) {
    let message_id = &seq.to_string();
    let messages = imap_session.fetch(message_id, "RFC822").unwrap();
    imap_session.store(message_id, "-FLAGS (\\Seen)").unwrap();

    let message = if let Some(m) = messages.iter().next() {
        m
    } else {
        return;
    };

    let body = message.body().expect("message did not have a body!");
    let body = std::str::from_utf8(body).expect("message was not valid utf-8");

    let parsed = parse_mail(body.as_bytes()).unwrap();

    let subject = parsed.headers.get_first_value("Subject").unwrap();
    println!("{:?}", subject);

    return;
}
