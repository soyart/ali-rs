pub fn file_exists<P>(path: P) -> bool
where
    P: AsRef<std::path::Path>,
{
    path.as_ref().exists()
}
