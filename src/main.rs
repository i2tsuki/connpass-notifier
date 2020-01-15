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
    let mail: String = env::var("IMAP_MAIL").expect("IMAP_MAIL is not given");

    let chunk: usize = 10 - 1;

    let tls: TlsConnector = native_tls::TlsConnector::builder().build().unwrap();

    let client: imap::Client<TlsStream<TcpStream>> =
        imap::connect((domain.as_str(), port), &domain, &tls).unwrap();

    let mut imap_session = client.login(user, password).unwrap();

    imap_session.select("INBOX").unwrap();

    let since: DateTime<Local> = Local::now() - Duration::hours(24 * 30);
    let from: &str = "no-reply@connpass.com";

    let sequences: HashSet<Seq> = imap_session
        .search(format!("FROM {} SINCE {}", from, since.format("%d-%b-%Y")))
        .unwrap();

    let mut v: Vec<String> = sequences.into_iter().map(|id| format!("{}", id)).collect();
    if v.len() > chunk {
        v.split_off(chunk);
    }
    let seqs: String = v.join(",");

    get_message_subject(&mut imap_session, seqs.as_str(), mail.as_str());

    imap_session.logout().unwrap();
}

fn get_message_subject<T: Read + Write>(
    imap_session: &mut imap::Session<T>,
    seqs: &str,
    mail: &str,
) {
    let messages: ZeroCopy<Vec<Fetch>> = imap_session.fetch(seqs, "RFC822").unwrap();
    imap_session.store(seqs, "-FLAGS (\\Seen)").unwrap();

    for message in messages.iter() {
        reduce_message_text(message, mail);

        imap_session
            .store(
                format!("{}", message.message).as_str(),
                "+FLAGS (\\Seen \\Deleted)",
            )
            .unwrap();
    }

    return;
}

fn reduce_message_text(message: &Fetch, mail: &str) {
    let message_id: String = format!("{}", message.message);

    let body: &[u8] = message.body().expect("message did not have a body!");
    let body: &str = std::str::from_utf8(body).expect("message was not valid utf-8");

    let parsed: mailparse::ParsedMail = parse_mail(body.as_bytes()).unwrap();

    let date: String = parsed.headers.get_first_value("Date").unwrap().unwrap();
    let unix: i64 = mailparse::dateparse(date.as_str()).unwrap();
    let subject: String = parsed.headers.get_first_value("Subject").unwrap().unwrap();

    let re_register_event: Regex = Regex::new(r"^.*さんが.*に参加登録しました。$").unwrap();
    let re_public_event1: Regex = Regex::new(r"^.*がイベント.*を公開しました$").unwrap();
    let re_public_event2 = Regex::new(r"^.*さんが.*を公開しました$").unwrap();
    let re_open_event = Regex::new(r"^.*の募集が開始されました$").unwrap();
    let re_document_add = Regex::new(r"^.*に資料が追加されました。$").unwrap();
    let re_event_message = Regex::new(r"^connpass イベント管理者からのメッセージ.*$").unwrap();

    if re_register_event.is_match(&subject)
        | re_public_event1.is_match(&subject)
        | re_public_event2.is_match(&subject)
        | re_open_event.is_match(&subject)
    {
        println!("{:<32}: {}", date, subject);
    } else if re_document_add.is_match(&subject) {
        println!("{:<32}: {}", date, subject);

        let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
        let mut s: String;

        match parsed.subparts[1].get_body_encoded().unwrap() {
            Body::SevenBit(body) | Body::EightBit(body) => {
                s = body.get_as_string().unwrap();
            }
            Body::Base64(body) => {
                s = body.get_decoded_as_string().unwrap();
            }
            _ => {
                println!("return");
                return;
            }
        }

        s = s.replace("\r", "")
            .replace(r#"
      <!-- フッタ文言部分 -->
      <table width="460" border="0" align="center" cellpadding="0" cellspacing="0" style="border-top:1px #CCC solid; padding-top:10px;">
        <tr>
          <td>
            <div style="font-size:10px; color:#333; line-height:16px;">
"#, "")
            .replace(format!(r#"
              {}宛てにメッセージが送信されました。<br>
              今後<a href="https://connpass.com/" target="_blank" style="color:#000;">connpass.com</a>からこのようなメールを受け取りたくない場合は、<a href="https://connpass.com/settings/" target="_blank" style="color:#000;">利用設定</a>から配信停止することができます。<br>
              ※ このメールに心当たりの無い方は、<a href="https://connpass.com/inquiry/" target="_blank" style="color:#000;">お問い合わせフォーム</a>からお問い合わせください。<br>
            </div>
            <div style="font-size:9px; color:#333; font-weight:bold; text-align:center; margin:15px auto 0;">Copyright © {} BeProud, Inc. All Rights Reserved.</div></td>
        </tr>
      </table>
"#, mail, Local.timestamp(unix, 0).format("%Y")).as_str(), "");

        f.write(&(s.as_bytes())).unwrap();
        f.flush().unwrap();

        print_mail_pdf("message.html", message_id.as_str());
    } else if re_event_message.is_match(&subject) {
        println!("{:<32}: {}", date, subject);

        let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
        let mut s: String;

        match parsed.subparts[1].get_body_encoded().unwrap() {
            Body::SevenBit(body) | Body::EightBit(body) => {
                s = body.get_as_string().unwrap();
            }
            Body::Base64(body) => {
                s = body.get_decoded_as_string().unwrap();
            }
            _ => {
                println!("return");
                return;
            }
        }

        f.write(&(s.as_bytes())).unwrap();
        f.flush().unwrap();

        print_mail_pdf("message.html", message_id.as_str());
    } else {
        println!("{}: {:<32}: {}", message.message, date, subject);

        if parsed.subparts.len() > 0 {
            match parsed.subparts[parsed.subparts.len() - 1]
                .get_body_encoded()
                .unwrap()
            {
                Body::SevenBit(body) | Body::EightBit(body) => {
                    let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
                    let s = body.get_as_string().unwrap();
                    f.write(&(s.as_bytes())).unwrap();
                    f.flush().unwrap();

                    print_mail_pdf("message.html", message_id.as_str());
                }
                Body::Base64(body) | Body::QuotedPrintable(body) => {
                    let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
                    let s = body.get_decoded_as_string().unwrap();
                    f.write(&(s.as_bytes())).unwrap();
                    f.flush().unwrap();

                    print_mail_pdf("message.html", message_id.as_str());
                }
                _ => {
                    return;
                }
            }
        } else {
            let mut f = BufWriter::new(fs::File::create("message.html").unwrap());
            let s = parsed.get_body().unwrap();

            f.write(&(s.as_bytes())).unwrap();
            f.flush().unwrap();

            print_mail_pdf("message.html", message_id.as_str());
        }
    }

    return;
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
