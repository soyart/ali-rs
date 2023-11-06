use std::io::Read;

use crate::errors::AliError;

const DELIMITER: &str = "://";

/// Synchronous network downloader
pub(crate) struct Downloader {
    proto: Protocol,
    url: String,
}

pub(crate) enum Protocol {
    Http, // Includes HTTPS
    Scp,
    Ftp,
    Sftp,
}

pub(crate) fn extract_proto_prefix(url: &str) -> Result<&str, AliError> {
    url.split_once(DELIMITER).map_or_else(
        || {
            Err(AliError::BadHookCmd(format!(
                "url {url} is missing delimiter '{DELIMITER}'"
            )))
        },
        |(prefix, _rest_url)| Ok(prefix),
    )
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http => write!(f, "http"),
            Self::Scp => write!(f, "scp"),
            Self::Ftp => write!(f, "ftp"),
            Self::Sftp => write!(f, "sftp"),
        }
    }
}

impl TryFrom<&str> for Protocol {
    type Error = AliError;

    fn try_from(prefix: &str) -> Result<Self, Self::Error> {
        match prefix {
            "http" | "https" => Ok(Self::Http),
            "scp" | "ssh" => Ok(Self::Scp),
            "ftp" => Ok(Self::Ftp),
            "sftp" => Ok(Self::Sftp),

            prefix => {
                Err(AliError::BadHookCmd(format!(
                    "unknown downloader protocol prefix {prefix}"
                )))
            }
        }
    }
}

impl Downloader {
    pub(crate) fn new(url: &str, proto: Protocol) -> Self {
        Self {
            url: url.to_string(),
            proto,
        }
    }

    pub(crate) fn new_from_url(url: &str) -> Result<Self, AliError> {
        let prefix = extract_proto_prefix(url)?;
        let proto = Protocol::try_from(prefix)?;

        match proto {
            Protocol::Http => Ok(Self::new(url, proto)),

            other_proto => {
                Err(AliError::NotImplemented(format!(
                    "downloader protocol {other_proto}"
                )))
            }
        }
    }

    pub(crate) fn get_string(&self) -> Result<String, AliError> {
        match self.proto {
            Protocol::Http => download_http_string(&self.url),
            ref other_proto => panic!("unexpected protocol: {other_proto}"),
        }
    }

    pub(crate) fn get_bytes(&self) -> Result<Vec<u8>, AliError> {
        match self.proto {
            Protocol::Http => download_http_bytes(&self.url),
            ref other_proto => panic!("unexpected protocol: {other_proto}"),
        }
    }
}

fn http_get(url: &str) -> Result<ureq::Response, AliError> {
    let resp = ureq::get(url).call().map_err(|err| {
        AliError::HookError(format!("failed to GET {url}: {err}"))
    })?;

    let status = resp.status();
    if !(200..=299).contains(&status) {
        return Err(AliError::HookError(format!("http status {status}")));
    }

    Ok(resp)
}

fn download_http_string(url: &str) -> Result<String, AliError> {
    let resp = http_get(url)?;

    resp.into_string().map_err(|err| {
        AliError::HookError(format!("body is not string: {err}"))
    })
}

fn download_http_bytes(url: &str) -> Result<Vec<u8>, AliError> {
    let resp = http_get(url)?;

    let mut r = resp.into_reader();
    let mut v = Vec::new();

    if let Err(err) = r.read_to_end(&mut v) {
        return Err(AliError::HookError(format!(
            "failed to read response bytes: {err}"
        )));
    }

    Ok(v)
}
