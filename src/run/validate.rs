use crate::errors::AliError;
use crate::manifest::validation;
use crate::manifest::Manifest;

pub(super) fn run(manifest_file: &str) -> Result<(), AliError> {
    let start = std::time::Instant::now();

    let manifest_yaml = std::fs::read_to_string(manifest_file)
        .map_err(|err| AliError::NoSuchFile(err, manifest_file.to_string()))?;

    let manifest = Manifest::from_yaml(&manifest_yaml)?;

    // @TODO: print validation result
    let _ = validation::validate(&manifest, true)?;
    println!("validation done in {:?}", start.elapsed());

    Ok(())
}
