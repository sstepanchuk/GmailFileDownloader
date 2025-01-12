# IMAP Email Attachment Downloader

This Rust program downloads email attachments (specifically images) from a specified sender using IMAP. It connects to the IMAP server securely using TLS, searches for emails from or to a given sender, and saves image attachments to a specified directory.

## Features
- Prompts for IMAP configuration (email, password, server, sender email, download directory) if a configuration file is not found.
- Connects securely to the IMAP server using TLS.
- Supports searching emails by sender (both "FROM" and "TO" fields).
- Downloads image attachments (JPEG/JPG) from the emails and saves them locally.
- Supports parallel processing of emails in batches for better performance.

## Dependencies
This program uses the following Rust crates:
- `async-imap`: For IMAP communication.
- `async-native-tls`: For secure TLS connections.
- `tokio`: For asynchronous runtime.
- `futures`: For asynchronous stream processing.
- `mailparse`: For parsing email messages.
- `serde`, `toml`: For configuration file handling.
- `dialoguer`: For interactive prompts.
- `anyhow`: For error handling.

## Configuration
The configuration is stored in a `config.toml` file, which includes the following fields:
```toml
email = "your-email@example.com"
password = "your-password"
sender = "sender@example.com"
server = "imap.example.com"
download_dir = "./downloaded_images"
```

### Example Configuration
```toml
email = "john.doe@gmail.com"
password = "password123"
sender = "newsletter@somecompany.com"
server = "imap.gmail.com"
download_dir = "./attachments"
```
If the file does not exist, the program will prompt the user to enter the required settings and save them.

## How to Run
1. Ensure Rust and Cargo are installed on your system.
2. Clone this repository or copy the code into a Rust project.
3. Run the following command to build and run the program:
   ```bash
   cargo run --release
   ```
4. Follow the prompts to enter your email configuration if `config.toml` does not exist.

## How It Works
1. **Connection**: The program establishes a secure IMAP connection using TLS.
2. **Mailbox Selection**: It lists available mailboxes and selects the one containing all emails.
3. **Search**: It searches for emails from or to the specified sender.
4. **Attachment Extraction**: It parses the emails and extracts image attachments (JPEG/JPG).
5. **Download**: The attachments are saved to the specified download directory.

## Limitations
- Currently, it only supports downloading image attachments with the MIME type `image/jpeg` or `image/jpg`.
- The IMAP server must support TLS for a secure connection.
- Authentication is done via email and password; OAuth is not supported.

## Future Enhancements
- Add support for OAuth authentication.
- Improve MIME type detection for a wider range of attachments.
- Add support for more mailbox flags and folder selection.

## License
This project is licensed under the MIT License. Feel free to use and modify it as per your needs.

## Contribution
Contributions are welcome! Please submit a pull request or open an issue for any feature requests or bug reports.

