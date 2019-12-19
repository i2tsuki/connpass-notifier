extern crate imap;
extern crate native_tls;

fn main() {
    let domain = "imap.example.com";
    let user = "";
    let password = "";

    let tls = native_tls::TlsConnector::builder().build().unwrap();

    let client = imap::connect((domain, 993), domain, &tls).unwrap();

    let mut imap_session = client.login(user, password).unwrap();

    println!("Hello, world!");
}
