extern crate imap;
extern crate native_tls;
extern crate regex;
extern crate serde;
extern crate serde_yaml;
extern crate tera;
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
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use tera::Context;
use tera::Tera;
use time::Duration;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct FilterYaml {
    filter: Vec<Filter>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Filter {
    rule: Vec<Rule>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Rule {
    remove: Pattern,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Pattern {
    exact: Option<Vec<String>>,
    regex: Option<Vec<String>>,
}

fn main() {
    let domain: String = env::var("IMAP_DOMAIN").expect("IMAP_DOMAIN is not given");
    let port: u16 = env::var("IMAP_PORT")
        .expect("IMAP_PORT is not given")
        .parse::<u16>()
        .expect("IMAP_PORT is not positive number");
    let user: String = env::var("IMAP_USER").expect("IMAP_USER is not given");
    let password: String = env::var("IMAP_PASSWORD").expect("IMAP_PASSWORD is not given");
    let mail: String = env::var("IMAP_MAIL").expect("IMAP_MAIL is not given");

    let mut chunk: usize = 10;
    chunk = chunk - 1;

    // Establish IMAP session to connect the server.
    let tls: TlsConnector = native_tls::TlsConnector::builder()
        .build()
        .expect("Cannot establish TLS connection to the server.");
    let client: imap::Client<TlsStream<TcpStream>> =
        imap::connect((domain.as_str(), port), &domain, &tls).expect("Cannot create IAMP client.");
    let mut imap_session = client
        .login(user, password)
        .expect("Cannot login to the server.");

    imap_session.select("INBOX").unwrap();

    let since: DateTime<Local> = Local::now() - Duration::hours(24 * 30);
    let from: &str = "no-reply@connpass.com";
    let sequences: HashSet<Seq> = imap_session
        .search(format!(
            "FROM {from} SINCE {since}",
            from = from,
            since = since.format("%d-%b-%Y")
        ))
        .unwrap();

    let mut v: Vec<String> = sequences.into_iter().map(|id| id.to_string()).collect();
    if v.len() > chunk {
        v.split_off(chunk);
    }
    let seqs: String = v.join(",");

    if seqs != "" {
        scrape_message(&mut imap_session, seqs.as_str(), mail.as_str());
    }

    imap_session.logout().unwrap();
}

fn scrape_message<T: Read + Write>(imap_session: &mut imap::Session<T>, seqs: &str, mail: &str) {
    let messages: ZeroCopy<Vec<Fetch>> = imap_session.fetch(seqs, "RFC822").unwrap();
    imap_session.store(seqs, "-FLAGS (\\Seen)").unwrap();

    for message in messages.iter() {
        reduce_message(message, mail);

        imap_session
            .store(message.message.to_string(), "+FLAGS (\\Seen \\Deleted)")
            .unwrap();
    }

    return;
}

fn reduce_message(message: &Fetch, mail: &str) {
    let filter_yaml_file = fs::File::open("filter.yaml").unwrap();
    let filters: FilterYaml = serde_yaml::from_reader(filter_yaml_file).unwrap();

    let message_id: String = message.message.to_string();

    let body: &[u8] = message.body().expect("message did not have a body!");
    let body: &str = std::str::from_utf8(body).expect("message was not valid utf-8");

    let parsed: mailparse::ParsedMail = parse_mail(body.as_bytes()).unwrap();

    let date: String = parsed.headers.get_first_value("Date").unwrap().unwrap();
    let unix: i64 = mailparse::dateparse(date.as_str()).unwrap();
    let subject: String = parsed.headers.get_first_value("Subject").unwrap().unwrap();
    let subject: &str = subject.as_str().trim();

    let re_register_event: Regex = Regex::new(r"^.*さんが.*に参加登録しました。$")
        .expect("does not compile regular expression");
    let re_public_event1: Regex =
        Regex::new(r"^.*がイベント.*を公開しました$").expect("does not compile regular expression");
    let re_public_event2 =
        Regex::new(r"^.*さんが.*を公開しました$").expect("does not compile regular expression");
    let re_open_event =
        Regex::new(r"^.*の募集が開始されました$").expect("does not compile regular expression");
    let re_document_add =
        Regex::new(r"^.*に資料が追加されました。$").expect("does not compile regular expression");
    let re_event_message = Regex::new(r"^connpass イベント管理者からのメッセージ.*$")
        .expect("does not compile regular expression");
    let re_group_message = Regex::new(r"^connpass グループ管理者からのメッセージ.*$")
        .expect("does not compile regular expression");

    let mut context = Context::new();
    context.insert("mail", mail);
    context.insert("year", &Local.timestamp(unix, 0).format("%Y").to_string());

    if re_register_event.is_match(&subject)
        | re_public_event1.is_match(&subject)
        | re_public_event2.is_match(&subject)
        | re_open_event.is_match(&subject)
    {
        println!("{} {:<32}: {}", message.message, date, subject);
    } else if re_document_add.is_match(&subject) {
        println!("{} {:<32}: {}", message.message, date, subject);

        let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
        let body: String;

        match parsed.subparts[1].get_body_encoded().unwrap() {
            Body::SevenBit(b) | Body::EightBit(b) => {
                body = b.get_as_string().unwrap();
            }
            Body::Base64(b) | Body::QuotedPrintable(b) => {
                body = b.get_decoded_as_string().unwrap();
            }
            _ => {
                println!("return");
                return;
            }
        }

        let lines: Vec<String> = reduce_message_body(body, context, filters);
        f.write(&(lines.join("\n").as_bytes())).unwrap();
        f.flush().unwrap();

        print_mail_pdf("message.html", message_id.as_str());
    } else if re_event_message.is_match(&subject) || re_group_message.is_match(&subject) {
        println!("{} {:<32}: {}", message.message, date, subject);

        let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
        let body: String;

        match parsed.subparts[1].get_body_encoded().unwrap() {
            Body::SevenBit(b) | Body::EightBit(b) => {
                body = b.get_as_string().unwrap();
            }
            Body::Base64(b) | Body::QuotedPrintable(b) => {
                body = b.get_decoded_as_string().unwrap();
            }
            _ => {
                println!("return");
                return;
            }
        }

        let lines: Vec<String> = reduce_message_body(body, context, filters);
        f.write(&(lines.join("\n").as_bytes())).unwrap();
        f.flush().unwrap();

        print_mail_pdf("message.html", message_id.as_str());
    } else {
        println!("{} {:<32}: {}", message.message, date, subject);

        let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
        let body: String;

        if parsed.subparts.len() > 0 {
            match parsed.subparts[parsed.subparts.len() - 1]
                .get_body_encoded()
                .unwrap()
            {
                Body::SevenBit(b) | Body::EightBit(b) => {
                    body = b.get_as_string().unwrap();
                }
                Body::Base64(b) | Body::QuotedPrintable(b) => {
                    body = b.get_decoded_as_string().unwrap();
                }
                _ => {
                    return;
                }
            }
        } else {
            body = parsed.get_body().unwrap();
        }

        f.write(&(body.as_bytes())).unwrap();
        f.flush().unwrap();
        print_mail_pdf("message.html", message_id.as_str());
    }

    return;
}

fn reduce_message_body(body: String, context: tera::Context, filters: FilterYaml) -> Vec<String> {
    let lines: Vec<String> = body
        .lines()
        .map(|line| {
            let l: String = line.to_string();

            for rule in &filters.filter[0].rule {
                if let Some(pattern) = &rule.remove.exact {
                    for exact in pattern {
                        let tpl: &str = exact;
                        let rendered = Tera::one_off(tpl, &context, true).unwrap();
                        if l.contains(&rendered) {
                            return "".to_string();
                        }
                    }
                }
                if let Some(pattern) = &rule.remove.regex {
                    for exact in pattern {
                        let tpl: &str = exact;
                        let rendered = Tera::one_off(tpl, &context, true).unwrap();
                        let re: Regex =
                            Regex::new(&rendered).expect("does not compile regular expression");
                        if re.is_match(&l) {
                            return "".to_string();
                        }
                    }
                }
            }
            return l;
        })
        .collect();
    return lines;
}

fn print_mail_pdf(file: &str, seq: &str) {
    let browser = Browser::default().unwrap();
    let tab = browser.wait_for_initial_tab().unwrap();

    let mut path = PathBuf::new();
    let cwd = std::env::current_dir().unwrap();
    path.push(cwd);
    path.push(file);

    tab.navigate_to(format!("file://{}", path.to_str().unwrap()).as_str())
        .unwrap()
        .wait_until_navigated()
        .unwrap();

    let bytes = tab.print_to_pdf(None).unwrap();

    let mut f = BufWriter::new(fs::File::create(format!("message{}.pdf", seq)).unwrap());
    f.write(&bytes).unwrap();
    f.flush().unwrap();
}
