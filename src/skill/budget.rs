use crate::error::{AruError, Result};

/// One shared structural budget bounds hashing of all candidate projections.
pub(super) struct SkillTreeStructuralBudget {
    max_depth: usize,
    max_directories: usize,
    max_entries: usize,
    directories: usize,
    entries: usize,
    exceeded: bool,
}

impl SkillTreeStructuralBudget {
    pub(super) fn new(max_depth: usize, max_directories: usize, max_entries: usize) -> Self {
        Self {
            max_depth,
            max_directories,
            max_entries,
            directories: 0,
            entries: 0,
            exceeded: false,
        }
    }

    pub(super) fn consume(&mut self, item: &walkdir::DirEntry) -> Result<()> {
        if self.entries >= self.max_entries {
            self.exceeded = true;
            return Err(limit_error("entries", self.max_entries));
        }
        self.entries += 1;
        if item.depth() > self.max_depth + 1 {
            self.exceeded = true;
            return Err(limit_error("depth", self.max_depth));
        }
        if item.file_type().is_dir() {
            if self.directories >= self.max_directories {
                self.exceeded = true;
                return Err(limit_error("directories", self.max_directories));
            }
            self.directories += 1;
        }
        Ok(())
    }

    pub(super) fn exceeded(&self) -> bool {
        self.exceeded
    }
}

fn limit_error(kind: &str, limit: usize) -> AruError {
    AruError::msg(format!("skill tree exceeded {kind} limit {limit}"))
}
