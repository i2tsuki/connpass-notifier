extern crate imap;
extern crate native_tls;
extern crate regex;
extern crate time;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::BufWriter;
use std::io::{Read, Write};
use std::path::PathBuf;

use chrono::prelude::*;
use headless_chrome::browser::Browser;
use imap::types::*;
use mailparse::body::Body;
use mailparse::*;
use native_tls::{TlsConnector, TlsStream};
use regex::Regex;
use std::net::TcpStream;
use time::Duration;

fn main() {
    let domain: String = env::var("IMAP_DOMAIN").expect("IMAP_DOMAIN is not given");
    let port: u16 = env::var("IMAP_PORT")
        .expect("IMAP_PORT is not given")
        .parse::<u16>()
        .expect("IMAP_PORT is not positive number");
    let user: String = env::var("IMAP_USER").expect("IMAP_USER is not given");
    let password: String = env::var("IMAP_PASSWORD").expect("IMAP_PASSWORD is not given");

    let chunk: usize = 10;

    let tls: TlsConnector = native_tls::TlsConnector::builder().build().unwrap();

    let client: imap::Client<TlsStream<TcpStream>> =
        imap::connect((domain.as_str(), port), &domain, &tls).unwrap();

    let mut imap_session = client.login(user, password).unwrap();

    imap_session.select("INBOX").unwrap();

    let since: DateTime<Local> = Local::now() - Duration::hours(24 * 30);

    let sequences: HashSet<Seq> = imap_session
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
    let message_id: &str = &seq.to_string();
    let messages: ZeroCopy<Vec<Fetch>> = imap_session.fetch(message_id, "RFC822").unwrap();
    imap_session.store(message_id, "-FLAGS (\\Seen)").unwrap();

    let message: &Fetch = if let Some(m) = messages.iter().next() {
        m
    } else {
        return;
    };

    let body: &[u8] = message.body().expect("message did not have a body!");
    let body: &str = std::str::from_utf8(body).expect("message was not valid utf-8");

    let parsed: mailparse::ParsedMail = parse_mail(body.as_bytes()).unwrap();

    let date: String = parsed.headers.get_first_value("Date").unwrap().unwrap();
    let subject: String = parsed.headers.get_first_value("Subject").unwrap().unwrap();

    let re: Regex = Regex::new(r"^.*さんが.*に参加登録しました。$").unwrap();
    if re.is_match(&subject) {
        println!("{:<32}: {}", date, subject);
        imap_session
            .store(message_id, "+FLAGS (\\Deleted)")
            .unwrap();
    }

    let re: Regex = Regex::new(r"^.*がイベント.*を公開しました$").unwrap();
    if re.is_match(&subject) {
        println!("{:<32}: {}", date, subject);
        imap_session
            .store(message_id, "+FLAGS (\\Deleted)")
            .unwrap();
    }

    let re: Regex = Regex::new(r"^.*さんが.*を公開しました$").unwrap();
    if re.is_match(&subject) {
        println!("{:<32}: {}", date, subject);
        imap_session
            .store(message_id, "+FLAGS (\\Deleted)")
            .unwrap();
    }

    let re: Regex = Regex::new(r"^.*に資料が追加されました。$").unwrap();
    if re.is_match(&subject) {
        println!("{:<32}: {}", date, subject);

        match parsed.subparts[1].get_body_encoded().unwrap() {
            Body::SevenBit(body) | Body::EightBit(body) => {
                let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
                let bytes = body.get_raw();
                f.write(&bytes).unwrap();
                f.flush().unwrap();

                let browser = Browser::default().unwrap();
                let tab = browser.wait_for_initial_tab().unwrap();

                let mut path = PathBuf::new();
                let cwd = std::env::current_dir().unwrap();
                path.push(cwd);
                path.push("message.html");

                tab.navigate_to(format!("file://{}", path.to_str().unwrap()).as_str())
                    .unwrap()
                    .wait_until_navigated()
                    .unwrap();

                let bytes = tab.print_to_pdf(None).unwrap();

                let mut f =
                    BufWriter::new(fs::File::create(format!("message{}.pdf", seq)).unwrap());
                f.write(&bytes).unwrap();
                f.flush().unwrap();
            }
            _ => {
                return;
            }
        }
        imap_session
            .store(message_id, "+FLAGS (\\Deleted)")
            .unwrap();
    }

    return;
}
