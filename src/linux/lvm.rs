use humanize_rs::bytes::Bytes;

use crate::errors::AliError;
use crate::manifest;

pub fn create_pv(pv: &str) -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}

pub fn create_vg(vg: &str, pvs: &[&str]) -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}

pub fn create_lv(lv: &str, pv: &str, size: Option<Bytes>) -> Result<(), AliError> {
    Err(AliError::NotImplemented)
}
