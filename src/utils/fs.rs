pub fn file_exists<P>(path: P) -> bool
where
    P: AsRef<std::path::Path>,
{
    std::fs::try_exists(path).map_err(|_| false).unwrap()
}
