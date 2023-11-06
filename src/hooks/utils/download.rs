use ureq;

use crate::errors::AliError;

pub(crate) struct Download {
    proto: Protocol,
    url: String,
}

pub(crate) enum Protocol {
    Http, // Includes HTTPS
    Scp,
    Ftp,
    Sftp,
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

impl Download {
    const DELIMITER: &'static str = "://";

    fn new(url: &str) -> Result<Self, AliError> {
        let splited = url.split_once(Self::DELIMITER);

        if splited.is_none() {
            return Err(AliError::BadHookCmd(format!(
                "missing protocol delimiter '{}' in url {url}",
                Self::DELIMITER,
            )));
        }

        let (prefix, _rest_url) = splited.unwrap();
        let proto = Protocol::try_from(prefix)?;

        match proto {
            Protocol::Http => {
                Ok(Self {
                    proto,
                    url: url.to_string(),
                })
            }

            other_proto => {
                Err(AliError::NotImplemented(format!(
                    "downloader protocol {other_proto}"
                )))
            }
        }
    }

    fn download(&self) -> Result<String, AliError> {
        match self.proto {
            Protocol::Http => download_http(&self.url),
            ref other_proto => panic!("unexpected protocol: {other_proto}"),
        }
    }
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

fn download_http(url: &str) -> Result<String, AliError> {
    let resp = ureq::get(url).call().map_err(|err| {
        AliError::HookError(format!("failed to GET {url}: {err}"))
    })?;

    let status = resp.status();
    if !(200..=299).contains(&status) {
        return Err(AliError::HookError(format!("http status {status}")));
    }

    resp.into_string().map_err(|err| {
        AliError::HookError(format!("body is not string: {err}"))
    })
}
