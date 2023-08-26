use crate::errors::AliError;
use crate::utils::shell;

pub fn mount(src: &str, opts: &str, dst: &str) -> Result<(), AliError> {
    shell::exec("mount", &[src, "-o", opts, dst])
}
