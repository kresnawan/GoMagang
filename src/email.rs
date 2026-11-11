//! Sending email. Mail goes out in the background, so a slow or unreachable SMTP server never
//! holds up or fails the request that caused it.

#[cfg(test)]
use std::sync::{Arc, Mutex};

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};

use crate::config::{SmtpConfig, SmtpTls};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub kepada: String,
    pub subjek: String,
    /// Plain text, for clients that do not show HTML.
    pub isi: String,
    pub html: Option<Html>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Html {
    pub isi: String,
    /// Images the HTML shows through `cid:<cid>`. They travel inside the email, as clients block
    /// remote images and most do not show SVG.
    pub gambar: Vec<GambarInline>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GambarInline {
    pub cid: &'static str,
    pub png: &'static [u8],
}

// Made once and shared through an Arc, so the size of the SMTP variant costs nothing.
#[allow(clippy::large_enum_variant)]
pub enum PengirimEmail {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        dari: Mailbox,
    },
    /// SMTP is not configured: mail is dropped.
    Nonaktif,
    /// Keeps the mail so tests can read it.
    #[cfg(test)]
    Uji(Arc<Mutex<Vec<Email>>>),
}

impl PengirimEmail {
    pub fn new(config: Option<&SmtpConfig>) -> Self {
        let Some(config) = config else {
            return Self::Nonaktif;
        };

        let dari = config
            .from
            .parse()
            .unwrap_or_else(|err| panic!("invalid SMTP_FROM {}: {err}", config.from));
        let mut builder = match config.tls {
            SmtpTls::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .expect("invalid SMTP_HOST"),
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .expect("invalid SMTP_HOST"),
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host),
        };
        if let Some(port) = config.port {
            builder = builder.port(port);
        }
        if let Some(username) = &config.username {
            builder = builder.credentials(Credentials::new(
                username.clone(),
                config.password.clone().unwrap_or_default(),
            ));
        }

        Self::Smtp {
            transport: builder.build(),
            dari,
        }
    }

    /// Sends the mail without waiting for it. A failure is only logged.
    pub fn kirim(&self, email: Email) {
        match self {
            Self::Smtp { transport, dari } => {
                let message = match build_message(dari, &email) {
                    Ok(message) => message,
                    Err(err) => {
                        eprintln!("failed to build email to {}: {err}", email.kepada);
                        return;
                    }
                };
                let transport = transport.clone();
                tokio::spawn(async move {
                    if let Err(err) = transport.send(message).await {
                        eprintln!("failed to send email to {}: {err}", email.kepada);
                    }
                });
            }
            Self::Nonaktif => {}
            #[cfg(test)]
            Self::Uji(terkirim) => terkirim.lock().unwrap().push(email),
        }
    }
}

fn build_message(dari: &Mailbox, email: &Email) -> Result<Message, String> {
    let kepada: Mailbox = email.kepada.parse().map_err(|err| format!("{err}"))?;
    let builder = Message::builder()
        .from(dari.clone())
        .to(kepada)
        .subject(&email.subjek);

    let result = match &email.html {
        None => builder
            .header(ContentType::TEXT_PLAIN)
            .body(email.isi.clone()),
        Some(html) => {
            let png = ContentType::parse("image/png").expect("valid content type");
            let related = html.gambar.iter().fold(
                MultiPart::related().singlepart(SinglePart::html(html.isi.clone())),
                |related, gambar| {
                    related.singlepart(
                        Attachment::new_inline(gambar.cid.to_string())
                            .body(gambar.png.to_vec(), png.clone()),
                    )
                },
            );
            builder.multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(email.isi.clone()))
                    .multipart(related),
            )
        }
    };
    result.map_err(|err| format!("{err}"))
}
