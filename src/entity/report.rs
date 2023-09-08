use serde_json::json;

use super::stage::StageActions;

#[derive(Debug)]
pub struct Report {
    pub location: String,
    pub summary: Box<StageActions>,
    pub duration: std::time::Duration,
}

impl Report {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "summary": self.summary,
            "elaspedTime": self.duration,
        })
    }

    pub fn to_json_string(&self) -> String {
        self.to_json().to_string()
    }
}

impl ToString for Report {
    fn to_string(&self) -> String {
        self.to_json_string()
    }
}

pub struct ValidationReport {
    pub block_devs: super::blockdev::BlockDevPaths,
}
