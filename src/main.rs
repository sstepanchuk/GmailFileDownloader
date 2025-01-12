use std::path::PathBuf;
use anyhow::{Result};
use async_imap::{self, Session};
use async_native_tls::{self, TlsStream};
use futures::TryStreamExt;
use mailparse;
use mailparse::MailHeaderMap;
use serde::{Serialize, Deserialize};
use dialoguer::Input;
use std::fs::File;
use std::io::Write;
use std::fs::read_to_string;
use async_std::net::TcpStream;
use std::collections::HashSet;

#[derive(Debug)]
struct EmailAttachment {
    filename: String,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct ImapConfig {
    email: String,
    password: String,
    sender: String,
    download_dir: PathBuf,
    server: String,
}

fn prompt_settings() -> Result<ImapConfig> {
    let email: String = Input::new()
        .with_prompt("Enter your email")
        .interact_text()?;

    let password: String = Input::new()
        .with_prompt("Enter your password")
        .interact_text()?;

    let sender: String = Input::new()
        .with_prompt("Enter the sender email")
        .interact_text()?;

    let server: String = Input::new()
        .with_prompt("Enter the IMAP server (e.g., imap.gmail.com)")
        .default("imap.gmail.com".to_string())
        .interact_text()?;

    let download_dir: String = Input::new()
        .with_prompt("Enter the download directory")
        .default("./downloaded_images".to_string())
        .interact_text()?;

    let config = ImapConfig {
        email,
        password,
        sender,
        server,
        download_dir: PathBuf::from(download_dir),
    };

    let toml_string = toml::to_string(&config)?;
    let mut file = File::create("config.toml")?;
    file.write_all(toml_string.as_bytes())?;

    Ok(config)
}

async fn connect_imap(config: &ImapConfig) -> Result<Session<TlsStream<TcpStream>>> {
    let imap_addr = (config.server.as_str(), 993);
    let tcp_stream = TcpStream::connect(imap_addr).await?;
    let tls = async_native_tls::TlsConnector::new();
    let tls_stream = tls.connect(config.server.as_str(), tcp_stream).await?;

    let client = async_imap::Client::new(tls_stream);
    println!("-- Connected to {}:{}", imap_addr.0, imap_addr.1);

    let imap_session = client.login(&config.email, &config.password).await.map_err(|e| e.0)?;
    println!("-- Logged in as {}", config.email);

    Ok(imap_session)
}

async fn save_attachment(attachment: &EmailAttachment, dir: &PathBuf) -> Result<()> {
    let path = dir.join(&attachment.filename);
    tokio::fs::write(&path, &attachment.data).await?;
    println!("Saved: {:?}", path);
    Ok(())
}

fn get_content_type(part: &mailparse::ParsedMail<'_>) -> Option<String> {
    part.headers.get_first_header("Content-Type")
        .map(|h| h.get_value().to_lowercase())
}

fn get_filename(part: &mailparse::ParsedMail<'_>) -> Option<String> {
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

    // Then try Content-Disposition
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

    // Finally try Content-ID
    if filename.is_none() {
        filename = part.headers.get_first_header("Content-ID")
            .map(|h| format!("image_{}.jpg", h.get_value().trim_matches(|c| c == '<' || c == '>')));
    }

    filename
}

fn extract_attachments(part: &mailparse::ParsedMail<'_>) -> Vec<EmailAttachment> {
    let mut attachments = Vec::new();

    // Check if this part is an image
    if let Some(content_type) = get_content_type(part) {
        if content_type.contains("image/") || content_type.contains("/jpeg") || content_type.contains("/jpg") {
            if let Some(filename) = get_filename(part) {
                if let Ok(data) = part.get_body_raw() {
                    attachments.push(EmailAttachment {
                        filename,
                        data,
                    });
                }
            }
        }
    }

    // Check subparts
    for subpart in &part.subparts {
        attachments.extend(extract_attachments(subpart));
    }

    attachments
}

async fn process_message(message_data: Vec<u8>, config: &ImapConfig) -> Result<()> {
    let parsed = mailparse::parse_mail(&message_data)?;
    let attachments = extract_attachments(&parsed);
    
    for attachment in attachments {
        save_attachment(&attachment, &config.download_dir).await?;
    }

    Ok(())
}

async fn download_attachments(config: &ImapConfig) -> Result<()> {
    tokio::fs::create_dir_all(&config.download_dir).await?;

    let mut imap_session = connect_imap(&config).await?;
    
    let folder_flag = "all";

    {
        let folders_stream = imap_session.list(Some(""), Some("*")).await?;
        let folders: Vec<_> = folders_stream.try_collect().await?;

        for folder in folders {
            if folder.attributes().iter().any(|flag| format!("{:?}", flag).to_lowercase().contains(folder_flag)) {
                println!("-- Found \"{}\" folder: {}", folder_flag, folder.name());
                imap_session.select(folder.name()).await?;
                break;
            }
        }
    }

    let from_query = format!("FROM \"{}\"", config.sender);
    let to_query = format!("TO \"{}\"", config.sender);
    
    let mut all_sequences = HashSet::new();

    if let Ok(sequences) = imap_session.search(&from_query).await {
        println!("Found {} emails FROM {}", sequences.len(), config.sender);
        all_sequences.extend(sequences);
    }

    if let Ok(sequences) = imap_session.search(&to_query).await {
        println!("Found {} emails TO {}", sequences.len(), config.sender);
        all_sequences.extend(sequences);
    }

    let sequences_vec: Vec<u32> = all_sequences.into_iter().collect();
    println!("Processing {} total emails", sequences_vec.len());

    // Process emails in parallel batches
    let batch_size = 10;
    for chunk in sequences_vec.chunks(batch_size) {
        let mut tasks = Vec::new();
        
        for &seq in chunk {
            let seq_str = seq.to_string();
            println!("\nProcessing email #{}", seq_str);
            
            let mut messages_stream = imap_session.fetch(seq_str, "RFC822").await?;
            
            while let Ok(Some(message)) = messages_stream.try_next().await {
                if let Some(body) = message.body() {
                    tasks.push(process_message(body.to_owned(), config));
                }
            }
        }
        
        futures::future::join_all(tasks).await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
    }

    println!("-- All messages processed, logging out");
    imap_session.logout().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = match read_to_string("config.toml") {
        Ok(content) => toml::from_str(&content)?,
        Err(_) => prompt_settings()?,
    };

    download_attachments(&config).await?;
    Ok(())
}