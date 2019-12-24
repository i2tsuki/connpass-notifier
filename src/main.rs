extern crate imap;
extern crate native_tls;
extern crate regex;
extern crate time;

use std::env;
use std::io::{Read, Write};

use chrono::prelude::*;
use imap::types::Seq;
use mailparse::*;
use regex::Regex;
use time::Duration;

fn main() {
    let domain = env::var("IMAP_DOMAIN").expect("IMAP_DOMAIN is not given");
    let port = env::var("IMAP_PORT")
        .expect("IMAP_PORT is not given")
        .parse::<u16>()
        .unwrap();
    let user = env::var("IMAP_USER").expect("IMAP_USER is not given");
    let password = env::var("IMAP_PASSWORD").expect("IMAP_PASSWORD is not given");

    let chunk = 10;

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = imap::connect((domain.as_str(), port), &domain, &tls).unwrap();

    let mut imap_session = client.login(user, password).unwrap();

    imap_session.select("INBOX").unwrap();

    let since: DateTime<Local> = Local::now() - Duration::hours(24 * 30);

    let sequences = imap_session
        .search(format!(
            "FROM no-reply@connpass.com SINCE {}",
            since.format("%d-%b-%Y")
        ))
        .unwrap();

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

    let date = parsed.headers.get_first_value("Date").unwrap().unwrap();
    let subject = parsed.headers.get_first_value("Subject").unwrap().unwrap();

    let re = Regex::new(r"^.*さんが.*に参加登録しました。$").unwrap();
    if re.is_match(&subject) {
        println!("{:<32}: {}", date, subject);
        imap_session
            .store(message_id, "+FLAGS (\\Deleted)")
            .unwrap();
    }

    let re = Regex::new(r"^.*がイベント.*を公開しました$").unwrap();
    if re.is_match(&subject) {
        println!("{:<32}: {}", date, subject);
        imap_session
            .store(message_id, "+FLAGS (\\Deleted)")
            .unwrap();
    }

    return;
}
