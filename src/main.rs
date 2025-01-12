use std::fs;
use std::path::PathBuf;
use imap;
use native_tls;
use mailparse;
use mailparse::MailHeaderMap;
use serde::{Serialize, Deserialize};
use dialoguer::Input;
use std::fs::File;
use std::io::Write;
use std::fs::read_to_string;

#[derive(Serialize, Deserialize)]
struct ImapConfig {
    email: String,
    password: String,
    sender: String,
    download_dir: PathBuf,
}

fn prompt_settings() -> ImapConfig {
    let email: String = Input::new()
        .with_prompt("Enter your email")
        .interact_text()
        .unwrap();

    let password: String = Input::new()
        .with_prompt("Enter your password")
        .interact_text()
        .unwrap();

    let sender: String = Input::new()
        .with_prompt("Enter the sender email")
        .interact_text()
        .unwrap();

    let download_dir: String = Input::new()
        .with_prompt("Enter the download directory")
        .default("./downloaded_images".to_string())
        .interact_text()
        .unwrap();

    let config = ImapConfig {
        email,
        password,
        sender,
        download_dir: PathBuf::from(download_dir),
    };

    let toml_string = toml::to_string(&config).unwrap();
    let mut file = File::create("config.toml").unwrap();
    file.write_all(toml_string.as_bytes()).unwrap();

    config
}

fn connect_imap(email: &str, password: &str) -> imap::Session<native_tls::TlsStream<std::net::TcpStream>> {
    let tls = native_tls::TlsConnector::builder().build().unwrap();
    let client = imap::connect(("imap.gmail.com", 993), "imap.gmail.com", &tls).unwrap();
    client.login(email, password).map_err(|e| e.0).unwrap()
}

fn save_attachment(data: &[u8], path: &PathBuf) {
    if let Err(e) = fs::write(path, data) {
        eprintln!("Error saving file {:?}: {}", path, e);
    } else {
        println!("Saved: {:?}", path);
    }
}

fn get_content_type(part: &mailparse::ParsedMail) -> Option<String> {
    part.headers.get_first_header("Content-Type")
        .map(|h| h.get_value().to_lowercase())
}

fn get_filename(part: &mailparse::ParsedMail) -> Option<String> {
    // Try Content-Type first
    let mut filename = part.headers.get_first_header("Content-Type")
        .and_then(|h| {
            let value = h.get_value();
            if value.contains("name=") {
                value.split("name=")
                    .nth(1)
                    .map(|f| f.trim_matches('"').to_string())
            } else {
                None
            }
        });

    // Then try Content-Disposition if no filename found
    if filename.is_none() {
        filename = part.headers.get_first_header("Content-Disposition")
            .and_then(|h| {
                let value = h.get_value();
                if value.contains("filename=") {
                    value.split("filename=")
                        .nth(1)
                        .map(|f| f.trim_matches('"').to_string())
                } else {
                    None
                }
            });
    }

    // If still no filename, try Content-ID
    if filename.is_none() {
        filename = part.headers.get_first_header("Content-ID")
            .map(|h| format!("image_{}.jpg", h.get_value().trim_matches(|c| c == '<' || c == '>')));
    }

    filename
}

fn process_email_part(part: &mailparse::ParsedMail, config: &ImapConfig) {
    // Check if this part is an image
    if let Some(content_type) = get_content_type(part) {
        if content_type.contains("image/") || content_type.contains("/jpeg") || content_type.contains("/jpg") {
            if let Some(filename) = get_filename(part) {
                println!("Found image attachment: {}", filename);
                if let Ok(data) = part.get_body_raw() {
                    let path = config.download_dir.join(&filename);
                    save_attachment(&data, &path);
                }
            }
        }
    }

    // Check subparts
    if part.subparts.len() > 0 {
        println!("Checking {} subparts", part.subparts.len());
        for subpart in &part.subparts {
            process_email_part(subpart, config);
        }
    }
}

fn download_attachments(config: ImapConfig) {
    // Create download directory
    fs::create_dir_all(&config.download_dir).unwrap_or_else(|e| {
        eprintln!("Error creating directory: {}", e);
    });

    // Connect to IMAP
    let mut imap = connect_imap(&config.email, &config.password);
    imap.select("[Gmail]/&BCMEQQRP- &BD8EPgRIBEIEMA-").unwrap();

    // Search for emails both from and to the address
    println!("Searching for emails...");
    
    let from_query = format!("FROM \"{}\"", config.sender);
    let to_query = format!("TO \"{}\"", config.sender);

    match imap.search(&from_query) {
        Ok(from_sequences) => println!("Found {} emails FROM {}", from_sequences.len(), config.sender),
        Err(e) => println!("Error searching FROM: {}", e),
    }

    match imap.search(&to_query) {
        Ok(to_sequences) => println!("Found {} emails TO {}", to_sequences.len(), config.sender),
        Err(e) => println!("Error searching TO: {}", e),
    }

    let combined_query = format!("OR (FROM \"{0}\") (TO \"{0}\")", config.sender);
    let sequences = match imap.search(&combined_query) {
        Ok(seq) => seq,
        Err(e) => {
            println!("Error with combined search: {}", e);
            println!("Falling back to FROM search only...");
            imap.search(&from_query).unwrap()
        }
    };
    
    println!("Processing {} total emails", sequences.len());

    // Process each email
    for seq in sequences {
        println!("\nProcessing email #{}", seq);
        if let Ok(messages) = imap.fetch(seq.to_string(), "RFC822") {
            if let Some(message) = messages.iter().next() {
                match mailparse::parse_mail(message.body().unwrap()) {
                    Ok(parsed) => process_email_part(&parsed, &config),
                    Err(e) => eprintln!("Error parsing email: {}", e),
                }
            }
        }
    }

    // Logout
    imap.logout().unwrap();
}

fn main() {
    let config = match read_to_string("config.toml") {
        Ok(content) => toml::from_str(&content).unwrap(),
        Err(_) => prompt_settings(),
    };

    download_attachments(config);
}