use nix::unistd::Uid;

/// Returns whether the current user is privileged
pub fn is_root() -> bool {
    Uid::effective().is_root()
}
